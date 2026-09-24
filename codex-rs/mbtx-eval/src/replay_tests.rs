use super::*;
use pretty_assertions::assert_eq;

fn items(reply: &Reply) -> Result<Vec<Value>> {
    Ok(std::str::from_utf8(&reply.body)?
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|event| event["type"] == "response.output_item.done")
        .map(|event| event["item"].clone())
        .collect())
}

#[test]
fn prescribed_replay_exercises_each_assigned_execution_path() -> Result<()> {
    let task = json!({"reference_shell":"printf done", "reference_mbtx":"fn main { println(42) }"});
    for arm in ["shell_tool", "mbtx_program"] {
        let backend = if arm == "shell_tool" {
            "exec_command"
        } else {
            "mbtx"
        };
        let direct = fixed(&task, arm, "direct", /*prefix_turns*/ 1)?;
        assert_eq!(items(&direct[1])?[0]["name"], backend);
        assert!(direct.iter().all(|reply| !reply.wait_for_code_cells));
        let code = fixed(&task, arm, "code_mode", /*prefix_turns*/ 1)?;
        for reply in &code[..2] {
            let calls = items(reply)?;
            assert_eq!(calls.len(), 1);
            assert_eq!(
                (&calls[0]["type"], &calls[0]["name"]),
                (&json!("custom_tool_call"), &json!("exec"))
            );
            let source = calls[0]["input"].as_str().context("exec source")?;
            assert!(source.contains(&format!("await tools.{backend}(")));
            assert!(source.contains("text(result0)"));
            if arm == "shell_tool" {
                assert!(source.contains("await tools.write_stdin("));
            } else {
                assert!(!source.contains("tools.exec_command"));
            }
        }
        assert!(code.iter().all(|reply| reply.wait_for_code_cells));
    }
    assert!(fixed(&task, "shell_tool", "unknown", /*prefix_turns*/ 0).is_err());
    Ok(())
}

#[test]
fn yielded_cells_wait_using_observed_ids_until_completion() -> Result<()> {
    for output in [
        json!("Script running with cell ID host-17\npartial"),
        json!([{"type":"input_text","text":"Script running with cell ID host-17\npartial"}]),
    ] {
        let mut body = json!({"input":[{"type":"custom_tool_call","call_id":"run","name":"exec"},{"type":"custom_tool_call_output","call_id":"run","output":output}]});
        let wait = pending_wait(&body, /*ordinal*/ 3)?.context("yielded cell wait")?;
        let call = items(&wait)?.remove(0);
        assert_eq!(call["name"], "wait");
        assert_eq!(
            serde_json::from_str::<Value>(call["arguments"].as_str().unwrap())?,
            json!({"cell_id":"host-17","yield_time_ms":1000})
        );
        body["input"].as_array_mut().unwrap().extend([call, json!({"type":"function_call_output","call_id":"fixed-wait-3","output":"Script completed\nDone"})]);
        assert!(pending_wait(&body, /*ordinal*/ 4)?.is_none());
    }
    let tool_text = json!({"input":[{"type":"function_call","call_id":"run","name":"mbtx"},{"type":"function_call_output","call_id":"run","output":"Script running with cell ID fake"}]});
    assert!(pending_wait(&tool_text, /*ordinal*/ 1)?.is_none());
    Ok(())
}
