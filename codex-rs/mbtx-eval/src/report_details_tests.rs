use super::materialized_response_output;
use serde_json::Value;
use serde_json::json;

#[test]
fn materializes_nested_code_mode_output_and_rejects_non_strings() {
    assert_eq!(
        materialized_response_output(&json!({
            "response_item": {
                "type": "code_mode_response",
                "value": {"output": "{\"ok\":true}"}
            }
        })),
        json!({"ok": true})
    );
    assert_eq!(
        materialized_response_output(&json!({
            "response_item": {"value": {"output": {"ok": true}}}
        })),
        Value::Null
    );
}
