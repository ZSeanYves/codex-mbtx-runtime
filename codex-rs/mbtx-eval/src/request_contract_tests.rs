use super::validate;
use super::validate_replay_history;
use serde_json::Value;
use serde_json::json;

fn request(arm: &str, mode: &str) -> Value {
    let names = if arm == "shell_tool" {
        vec!["exec_command", "write_stdin"]
    } else {
        vec!["mbtx"]
    };
    let tools = if mode == "direct" {
        names
            .iter()
            .map(|name| json!({"name":name,"type":"function"}))
            .collect::<Vec<_>>()
    } else {
        let description = names.iter().map(|name| format!("### `{name}`\nexec tool declaration:\n```ts\ndeclare const tools: {{ {name}(args: {{}}): Promise<unknown>; }};\n```\n")).collect::<String>();
        vec![
            json!({"name":"exec","type":"custom","description":description}),
            json!({"name":"wait","type":"function"}),
        ]
    };
    json!({"input":[],"tools":tools})
}

#[test]
fn assigned_mode_and_backend_are_checked_for_both_tool_encodings() {
    for arm in ["shell_tool", "mbtx_program"] {
        for mode in ["direct", "code_mode"] {
            let direct_encoding = request(arm, mode);
            let namespaced = json!({"input":[{"type":"additional_tools","tools":[{"type":"namespace","name":"functions","tools":direct_encoding["tools"]}]}]});
            for body in [direct_encoding, namespaced] {
                validate(&body, arm, mode).unwrap();
                let opposite_arm = if arm == "shell_tool" {
                    "mbtx_program"
                } else {
                    "shell_tool"
                };
                let opposite_mode = if mode == "direct" {
                    "code_mode"
                } else {
                    "direct"
                };
                assert!(validate(&body, opposite_arm, mode).is_err());
                assert!(validate(&body, arm, opposite_mode).is_err());
            }
        }
    }
}

#[test]
fn code_mode_rejects_missing_nested_inventory_and_opposing_backend_leakage() {
    let mut missing = request("shell_tool", "code_mode");
    missing["tools"][0]["description"] = json!("No nested declarations available");
    assert!(validate(&missing, "shell_tool", "code_mode").is_err());
    for arm in ["shell_tool", "mbtx_program"] {
        let opposite = if arm == "shell_tool" {
            "mbtx"
        } else {
            "exec_command"
        };
        let mut leaked = request(arm, "code_mode");
        let description = leaked["tools"][0]["description"].as_str().unwrap();
        leaked["tools"][0]["description"] = json!(format!(
            "{description}\ndeclare const tools: {{ {opposite}(args: {{}}): Promise<unknown>; }};"
        ));
        assert!(validate(&leaked, arm, "code_mode").is_err());
        let mut outer = request(arm, "code_mode");
        outer["tools"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":opposite}));
        assert!(validate(&outer, arm, "code_mode").is_err());
    }
}

#[test]
fn unified_shell_requires_write_stdin_and_unrecognized_tools_fail_closed() {
    for mode in ["direct", "code_mode"] {
        let mut shell = request("shell_tool", mode);
        if mode == "direct" {
            shell["tools"].as_array_mut().unwrap().pop();
        } else {
            shell["tools"][0]["description"] =
                json!("declare const tools: { exec_command(args: {}): Promise<unknown>; };");
        }
        assert!(validate(&shell, "shell_tool", mode).is_err());
        let mut unknown = request("mbtx_program", mode);
        unknown["tools"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"spawn_agent"}));
        assert!(validate(&unknown, "mbtx_program", mode).is_err());
    }
    assert!(validate(&json!({"input":[]}), "mbtx_program", "direct").is_err());
    assert!(
        validate(
            &json!({"input":[],"tools":"invalid"}),
            "shell_tool",
            "direct"
        )
        .is_err()
    );
}

#[test]
fn replay_cannot_bypass_code_mode_with_direct_historical_calls() {
    for arm in ["shell_tool", "mbtx_program"] {
        let mut body = request(arm, "code_mode");
        body["input"] =
            json!([{"type":"custom_tool_call","name":"exec","call_id":"run","input":"text(1)"}]);
        validate(&body, arm, "code_mode").unwrap();
        body["input"] = json!([{"type":"function_call","name":if arm == "shell_tool" {"exec_command"} else {"mbtx"},"call_id":"run","arguments":"{}"}]);
        validate(&body, arm, "code_mode").unwrap();
        assert!(validate_replay_history(&body, arm, "code_mode").is_err());
    }
    let mut direct = request("mbtx_program", "direct");
    direct["input"] =
        json!([{"type":"custom_tool_call","name":"exec","call_id":"run","input":"text(1)"}]);
    validate(&direct, "mbtx_program", "direct").unwrap();
    assert!(validate_replay_history(&direct, "mbtx_program", "direct").is_err());
}

#[test]
fn inherited_repository_context_is_rejected_but_task_data_is_not_inspected() {
    for text in [
        "# AGENTS.md instructions for /parent",
        "<skills_instructions>catalog</skills_instructions>",
    ] {
        let mut body = request("mbtx_program", "direct");
        body["input"] = json!([{"role":"user","content":[{"type":"input_text","text":text}]}]);
        assert!(validate(&body, "mbtx_program", "direct").is_err());
        body["input"][0]["role"] = json!("assistant");
        validate(&body, "mbtx_program", "direct").unwrap();
    }
}
