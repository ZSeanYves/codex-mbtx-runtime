use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::evidence::read_json;

#[derive(Clone)]
pub(crate) struct Reply {
    pub headers: axum::http::HeaderMap,
    pub status: u16,
    pub content_type: String,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
    pub disconnect: bool,
    pub delay_ms: u64,
    pub wait_for_code_cells: bool,
}

fn sse(items: &[Value], response_id: &str) -> Reply {
    let mut events = vec![json!({"type":"response.created","response":{"id":response_id}})];
    for item in items {
        events.push(json!({"type":"response.output_item.done","item":item}));
    }
    events.push(json!({"type":"response.completed","response":{"id":response_id,"output":items,"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}));
    Reply {
        status: 200,
        headers: axum::http::HeaderMap::new(),
        content_type: "text/event-stream".into(),
        retry_after: None,
        body: events
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>()
            .into_bytes(),
        disconnect: false,
        delay_ms: 0,
        wait_for_code_cells: false,
    }
}

pub(crate) fn fault(name: &str) -> Reply {
    let status = match name {
        "429" => 429,
        "500" => 500,
        _ => 200,
    };
    Reply {
        status,
        headers: axum::http::HeaderMap::new(),
        content_type: if status == 200 {
            "text/event-stream"
        } else {
            "application/json"
        }
        .into(),
        retry_after: Some("0".into()),
        body: if status == 200 {
            b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"fault\"}}\n\n".to_vec()
        } else {
            b"{\"error\":{\"message\":\"controlled offline fault\"}}".to_vec()
        },
        disconnect: name == "disconnect",
        delay_ms: if name == "stall" { 60_000 } else { 0 },
        wait_for_code_cells: false,
    }
}

pub(crate) fn fixed(
    task: &Value,
    arm: &str,
    execution_mode: &str,
    prefix_turns: usize,
) -> Result<Vec<Reply>> {
    anyhow::ensure!(
        matches!(arm, "mbtx_program" | "shell_tool"),
        "unknown arm: {arm}"
    );
    anyhow::ensure!(
        matches!(execution_mode, "direct" | "code_mode"),
        "unknown execution mode: {execution_mode}"
    );
    let arguments = if arm == "mbtx_program" {
        let program = crate::replay_program::source(task)?;
        let source = if let Some(name) = crate::submission_contract::required_source(task, arm)? {
            // The delivered program depends on inputs alone. Only its initial
            // interactive invocation writes the exact source for validation.
            program.replacen(
                "async fn main {",
                &format!(
                    "async fn main {{\n  @fs.write_file({}, {})",
                    serde_json::to_string(&name)?,
                    serde_json::to_string(&program)?
                ),
                1,
            )
        } else {
            program
        };
        anyhow::ensure!(
            source.len() <= crate::submission_contract::SOURCE_LIMIT_BYTES as usize,
            "replay invocation exceeds source limit"
        );
        json!({"source":source,"max_output_bytes":4096})
    } else {
        let script = task["reference_shell"]
            .as_str()
            .context("frozen Shell reference")?;
        let quoted = format!("'{}'", script.replace('\'', "'\\''"));
        let command = if let Some(name) = crate::submission_contract::required_source(task, arm)? {
            anyhow::ensure!(name == "solution.sh", "unsupported Shell source contract");
            format!("set -eu\nprintf '%s' {quoted} > solution.sh\nsh solution.sh")
        } else {
            script.to_owned()
        };
        json!({"cmd":command,"login":false,"max_output_tokens":1024})
    };
    // Deliberately prescribed solutions validate transport, tool execution and
    // independent negative oracle tests. They are never research samples.
    let mut replies = Vec::new();
    for turn in 0..prefix_turns {
        let (name, args) = if arm == "mbtx_program" {
            (
                "mbtx",
                json!({"source":"fn main { println(\"fresh execution 雪\") }"}),
            )
        } else {
            (
                "exec_command",
                json!({"cmd":"printf 'fresh execution 雪\\n'","login":false}),
            )
        };
        let calls = [
            (format!("prefix-{turn}-run"), name, args),
            (
                format!("prefix-{turn}-read"),
                "read_resource",
                json!({"resource_id":"reference:moonbit","offset":0,"max_bytes":128}),
            ),
        ];
        replies.push(sse(
            &tool_calls(&calls, execution_mode),
            &format!("prefix-{turn}"),
        ));
    }
    replies.extend([
        sse(
            &tool_calls(&[("fixed-output".into(), if arm == "mbtx_program" { "mbtx" } else { "exec_command" }, arguments)], execution_mode),
            "fixed-tool",
        ),
        sse(
            &[
                json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"Task completed."}]}),
            ],
            "fixed-final",
        ),
    ]);
    for reply in &mut replies {
        reply.wait_for_code_cells = execution_mode == "code_mode";
    }
    Ok(replies)
}

fn tool_calls(calls: &[(String, &str, Value)], execution_mode: &str) -> Vec<Value> {
    if execution_mode == "direct" {
        return calls.iter().map(|(id, name, args)| json!({"type":"function_call","call_id":id,"name":name,"arguments":args.to_string()})).collect();
    }
    let mut code = String::from("// @exec: {\"yield_time_ms\": 1000}\n");
    for (index, (_, name, args)) in calls.iter().enumerate() {
        code.push_str(&format!(
            "let result{index} = await tools.{name}({args}); text(result{index});\n"
        ));
        if *name == "exec_command" {
            code.push_str(&format!("while (result{index}.session_id != null) {{ result{index} = await tools.write_stdin({{session_id: result{index}.session_id, chars: \"\", yield_time_ms: 1000}}); text(result{index}); }}\n"));
        }
    }
    vec![json!({"type":"custom_tool_call","call_id":calls[0].0,"name":"exec","input":code})]
}

/// Continue a yielded prescribed cell before advancing its replay schedule.
/// Recorded provider responses never use this synthesis.
pub(crate) fn pending_wait(body: &Value, ordinal: usize) -> Result<Option<Reply>> {
    let input = body["input"].as_array().context("replay request input")?;
    let Some(output) = input.iter().rev().find(|item| {
        matches!(
            item["type"].as_str(),
            Some("custom_tool_call_output" | "function_call_output")
        )
    }) else {
        return Ok(None);
    };
    if !input.iter().any(|item| {
        item["call_id"] == output["call_id"]
            && matches!(item["name"].as_str(), Some("exec" | "wait"))
    }) {
        return Ok(None);
    }
    let text = output["output"].as_str().or_else(|| {
        output["output"]
            .as_array()
            .and_then(|parts| parts.first())
            .and_then(|part| part["text"].as_str())
    });
    let Some(cell_id) = text
        .and_then(|text| text.strip_prefix("Script running with cell ID "))
        .and_then(|rest| rest.lines().next())
    else {
        return Ok(None);
    };
    anyhow::ensure!(!cell_id.is_empty(), "yielded replay cell id is empty");
    Ok(Some(sse(
        &[
            json!({"type":"function_call","call_id":format!("fixed-wait-{ordinal}"),"name":"wait","arguments":json!({"cell_id":cell_id,"yield_time_ms":1000}).to_string()}),
        ],
        &format!("fixed-wait-{ordinal}"),
    )))
}

pub(crate) fn recorded(attempt: &Path) -> Result<Vec<Reply>> {
    let mut directories = std::fs::read_dir(attempt.join("http"))?
        .map(|entry| entry.map(|v| v.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    directories.sort();
    directories.into_iter().map(|path| {
        let headers=read_json(&path.join("headers.json"))?;
        let result=read_json(&path.join("result.json"))?;
        anyhow::ensure!(result["complete"]==true,"cannot execute a truncated recorded stream as a complete replay");
        let body=std::fs::read(path.join("response.body"))?;
        let source=attempt.parent().and_then(Path::parent).context("source run")?;
        anyhow::ensure!(!String::from_utf8_lossy(&body).contains(source.to_string_lossy().as_ref()),"recorded response references the source run; portable replay requires relative workspace paths. Original evidence is retained without rewriting commands.");
        Ok(Reply { headers:crate::http_headers::recorded(&headers["protocol_headers"] )?,status:headers["status_code"].as_u64().context("recorded status")? as u16,content_type:headers["content_type"].as_str().context("content type")?.to_owned(),retry_after:headers["retry_after"].as_str().map(str::to_owned),body,disconnect:false,delay_ms:0,wait_for_code_cells:false })
    }).collect()
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
