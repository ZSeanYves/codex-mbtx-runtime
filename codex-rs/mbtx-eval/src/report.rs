use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::analysis::Analysis;
use crate::evidence::json_new;
use crate::evidence::read_json;
use crate::evidence::verify;
use crate::evidence::write_new;

pub(crate) fn response_terminal(body: &str) -> Option<&'static str> {
    let mut data = String::new();
    let mut terminal = None;
    for line in body.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                match value["type"].as_str() {
                    Some("response.completed") => terminal = Some("completed"),
                    Some("response.failed" | "error") => terminal = Some("failed"),
                    _ => (),
                }
            }
            data.clear();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.trim_start());
            data.push('\n');
        }
    }
    terminal
}

pub(crate) fn find_attempt(root: &Path, pair: &Value, arm: &str) -> Result<PathBuf> {
    for directory in directories(&root.join("attempts"))? {
        let assignment = read_json(&directory.join("assignment.json"))?;
        if assignment["pair_id"] == *pair && assignment["arm"] == arm {
            return Ok(directory);
        }
    }
    anyhow::bail!("no recorded attempt for {pair} {arm}")
}

pub(crate) fn directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(root)?
        .map(|v| v.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.is_dir());
    paths.sort();
    Ok(paths)
}

pub(crate) async fn assess_attempt(
    directory: &Path,
    manifest: &Value,
    analysis: &mut Analysis,
) -> Result<Value> {
    let assignment = read_json(&directory.join("assignment.json")).unwrap_or(Value::Null);
    let attempt_id = directory
        .file_name()
        .context("attempt directory name")?
        .to_string_lossy();
    let task = manifest["protocol"]["tasks"]
        .as_array()
        .context("tasks")?
        .iter()
        .find(|t| t["id"] == assignment["task_id"])
        .cloned()
        .unwrap_or(Value::Null);
    let mut errors = Vec::new();
    let assignment_valid = assignment["attempt_id"] == attempt_id.as_ref()
        && matches!(
            assignment["arm"].as_str(),
            Some("shell_tool" | "mbtx_program")
        )
        && manifest["schedule"].as_array().is_some_and(|pairs| {
            pairs.iter().any(|p| {
                p["pair_id"] == assignment["pair_id"] && p["task_id"] == assignment["task_id"]
            })
        })
        && !task.is_null();
    if !assignment_valid {
        errors
            .push("assignment absent, incomplete, or inconsistent with the frozen schedule".into());
    }
    let mut trace = Value::Null;
    if directory.join("trace").exists() {
        let bundles = directories(&directory.join("trace"))?;
        if bundles.len() == 1 {
            match codex_rollout_trace::replay_bundle(&bundles[0]) {
                Ok(value) => trace = serde_json::to_value(value)?,
                Err(error) => errors.push(format!("native reduction: {error}")),
            }
        } else {
            errors.push(format!(
                "expected one native capture, found {}",
                bundles.len()
            ));
        }
    }
    let outcome = read_json(&directory.join("outcome.json")).unwrap_or(Value::Null);
    let snapshot = read_json(&directory.join("snapshot.json"))
        .unwrap_or_else(|_| json!({"files":{},"errors":["snapshot absent or incomplete"]}));
    let mut exchanges = Vec::new();
    if directory.join("http").exists() {
        for request in directories(&directory.join("http"))? {
            let mut value = read_json(&request.join("result.json")).unwrap_or_else(
                |_| json!({"complete":false,"status_code":null,"transport_error":null}),
            );
            value["started"] = read_json(&request.join("started.json")).unwrap_or(Value::Null);
            value["headers"] = read_json(&request.join("headers.json")).unwrap_or(Value::Null);
            let sse = value["headers"]["content_type"]
                .as_str()
                .is_some_and(|v| v.starts_with("text/event-stream"));
            value["sse_terminal"] = if sse {
                json!(
                    fs::read_to_string(request.join("response.body"))
                        .ok()
                        .and_then(|s| response_terminal(&s))
                )
            } else {
                Value::Null
            };
            value["sse_expected"] = json!(sse);
            value["evidence"] = json!(request.strip_prefix(directory)?.to_string_lossy());
            exchanges.push(value);
        }
    }
    let integrity = if !assignment_valid || manifest["manifest_integrity"] == false {
        json!(false)
    } else if directory.join("seal.json").exists() {
        json!(verify(directory).unwrap_or(false))
    } else {
        Value::Null
    };
    let facts = json!({"attempt_id":attempt_id,"pair_id":assignment["pair_id"],"arm":assignment["arm"],"trace":trace,"snapshot":snapshot,"outcome":outcome,"execution_error":read_json(&directory.join("execution-error.json")).unwrap_or(Value::Null),"exchanges":exchanges,"integrity":integrity,"evidence":{"directory":format!("attempts/{attempt_id}"),"errors":errors}});
    analysis
        .query(json!({"op":"attempt","task":task,"facts":facts}))
        .await
}

pub(crate) async fn assess(root: &Path, analysis: &mut Analysis) -> Result<Vec<Value>> {
    let mut manifest = read_json(&root.join("run.json"))?;
    manifest["manifest_integrity"] = json!(crate::evidence::verify_manifest(root).unwrap_or(false));
    let mut assigned = Vec::new();
    for directory in directories(&root.join("attempts"))? {
        let meta = read_json(&directory.join("assignment.json")).unwrap_or(Value::Null);
        assigned.push((meta["assigned_ms"].as_u64().unwrap_or(0), directory));
    }
    assigned.sort();
    let mut result = Vec::new();
    for (_, directory) in assigned {
        result.push(assess_attempt(&directory, &manifest, analysis).await?);
    }
    Ok(result)
}

pub(crate) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn cell(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn render(model: &Value, format: &str) -> Result<String> {
    if format == "json" {
        return Ok(serde_json::to_string_pretty(model)?);
    }
    let mut rows = vec![vec![
        "Task".into(),
        "Arm".into(),
        "Status".into(),
        "Oracle".into(),
        "Accepted steps".into(),
        "HTTP sends".into(),
        "Tool calls".into(),
        "Tool errors".into(),
        "Repair steps (unknown)".into(),
        "Evidence".into(),
    ]];
    for a in model["attempts"].as_array().context("attempts")? {
        rows.push(vec![
            cell(&a["task_id"]),
            cell(&a["arm"]),
            cell(&a["status"]),
            cell(&a["oracle"]["success"]),
            cell(&a["accounting"]["metrics"]["agent_steps"]),
            cell(&a["accounting"]["metrics"]["model_requests"]),
            cell(&a["accounting"]["metrics"]["tool_calls"]),
            cell(&a["accounting"]["metrics"]["tool_errors"]),
            cell(&a["accounting"]["metrics"]["repair_steps"]),
            cell(&a["evidence"]["directory"]),
        ]);
    }
    if format == "csv" {
        return Ok(rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| format!("\"{}\"", v.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n");
    }
    let summary = format!(
        "Run {} | {} | {} | partial={}\nShell: {} successful / {} assigned. MBTX: {} successful / {} assigned.\nComparable successful pairs: {}. Mean MBTX minus Shell steps: {}.\nPilot only; fixed replay is not research evidence. Unknown values remain null.",
        cell(&model["run_id"]),
        cell(&model["mode"]),
        cell(&model["platform"]),
        model["partial"],
        model["itt"]["shell_tool"]["successes"],
        model["itt"]["shell_tool"]["assigned"],
        model["itt"]["mbtx_program"]["successes"],
        model["itt"]["mbtx_program"]["assigned"],
        model["conditional"]["pairs"],
        model["conditional"]["mean_step_difference"]
    );
    if format == "md" {
        let table = rows
            .iter()
            .map(|row| {
                format!(
                    "| {} |",
                    row.iter()
                        .map(|v| v.replace('|', "\\|").replace(['\r', '\n'], " "))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            })
            .collect::<Vec<_>>();
        return Ok(format!(
            "# Programmable MBTX pilot report\n\n{summary}\n\n{}\n|{}|\n{}\n\nFirst trajectory divergences and success-by-step curves are in report.json; their source sequences identify the raw observations. Divergence does not establish causation.\n",
            table[0],
            vec!["---"; rows[0].len()].join("|"),
            table[1..].join("\n")
        ));
    }
    anyhow::ensure!(format == "html", "unsupported report format");
    let mut html = format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Programmable MBTX pilot</title><style>body{{font:16px system-ui;margin:2rem;max-width:1200px}}table{{border-collapse:collapse}}td,th{{padding:.6rem;border:1px solid #bbb;text-align:left}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}a{{color:#075bb0}}</style><h1>Programmable MBTX pilot</h1><pre>{}</pre><p>Interactive trace inspection: import the accompanying OTLP JSON into SigNoz. This summary is rebuilt solely from retained evidence.</p><table>",
        escape(&summary)
    );
    for (i, row) in rows.iter().enumerate() {
        html.push_str("<tr>");
        for (j, value) in row.iter().enumerate() {
            let tag = if i == 0 { "th" } else { "td" };
            let text = if i > 0 && j == row.len() - 1 {
                format!(
                    "<a href=\"../../{}/assignment.json\">Raw evidence</a>",
                    escape(value)
                )
            } else {
                escape(value)
            };
            html.push_str(&format!("<{tag}>{text}</{tag}>"));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table><p><a href=\"report.json\">Full analysis JSON</a></p></html>");
    Ok(html)
}

pub(crate) async fn generate(
    root: &Path,
    bundle: &Path,
    analysis: &mut Analysis,
    format: &str,
) -> Result<PathBuf> {
    let mut manifest = read_json(&root.join("run.json"))?;
    manifest["manifest_integrity"] = json!(crate::evidence::verify_manifest(root).unwrap_or(false));
    let attempts = assess(root, analysis).await?;
    let latest = fs::read_dir(root)?
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("collection-session-")
        })
        .filter_map(|entry| read_json(&entry.path()).ok())
        .max_by_key(|value| value["ended_ms"].as_u64());
    manifest["collection_stop_reason"] = latest
        .map(|v| v["stop_reason"].clone())
        .unwrap_or(Value::Null);
    let model = analysis
        .query(json!({"op":"report","manifest":manifest,"attempts":attempts}))
        .await?;
    let output = root.join("reports").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&output)?;
    json_new(
        &output.join("analysis-input.json"),
        &json!({"manifest":manifest,"attempts":attempts,"analyzer_bundle":read_json(&bundle.join("bundle.json"))?}),
    )?;
    let formats = if format == "all" {
        vec!["json", "csv", "md", "html"]
    } else if format == "json" {
        vec![format]
    } else {
        vec!["json", format]
    };
    for format in formats {
        write_new(
            &output.join(format!("report.{format}")),
            render(&model, format)?.as_bytes(),
        )?;
    }
    crate::observe::export_file(root, &output, &model)?;
    crate::evidence::seal(&output)?;
    eprintln!("[eval] report: {}", output.display());
    Ok(output)
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
