use super::validate;
use serde_json::json;

#[test]
fn checks_direct_and_namespaced_tools_without_silently_skipping_absent_schemas() {
    for arm in ["shell_tool", "mbtx_program"] {
        let name = if arm == "shell_tool" {
            "exec_command"
        } else {
            "mbtx"
        };
        let tools = json!([
            {"type":"function","name":name},
            {"type":"custom","name":"apply_patch"},
            {"type":"custom","name":"exec"},
            {"type":"function","name":"wait"}
        ]);
        for mut body in [
            json!({"input":[],"tools":tools}),
            json!({"input":[{"type":"additional_tools","tools":[{"type":"namespace","name":"functions","tools":tools}]}]}),
        ] {
            validate(&body, arm).unwrap();
            let other = if arm == "shell_tool" {
                "mbtx_program"
            } else {
                "shell_tool"
            };
            assert!(validate(&body, other).is_err());
            body["tools"] = json!([{"type":"function","name":"spawn_agent"}]);
            assert!(validate(&body, arm).is_err());
        }
    }
    assert!(validate(&json!({"input":[]}), "mbtx_program").is_err());
    for arm in ["shell_tool", "mbtx_program"] {
        assert!(
            validate(
                &json!({"input":[],"tools":[{"name":"exec"},{"name":"wait"}]}),
                arm
            )
            .is_err()
        );
    }
    assert!(validate(&json!({"input":[],"tools":"invalid"}), "shell_tool").is_err());
    assert!(validate(&json!({"input":[{"type":"additional_tools","tools":[{"type":"namespace","name":"collaboration","tools":[{"name":"spawn_agent"}]}]}]}), "mbtx_program").is_err());
}

#[test]
fn inherited_repository_context_is_rejected_but_task_data_is_not_inspected() {
    for text in [
        "# AGENTS.md instructions for /parent",
        "<skills_instructions>catalog</skills_instructions>",
    ] {
        let mut body = json!({"tools":[{"name":"mbtx"}],"input":[{"role":"user","content":[{"type":"input_text","text":text}]}]});
        assert!(validate(&body, "mbtx_program").is_err());
        body["input"][0]["role"] = json!("assistant");
        validate(&body, "mbtx_program").unwrap();
    }
}
