use super::materialized_response_output;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

#[test]
fn materializes_direct_and_code_mode_results_without_discarding_compilation_evidence() {
    let mbtx = json!({"status":"success","stage":"completed","build":{"duration_ms":17},"run":{"exit_code":0},"cache":"miss"});
    for result in [
        json!({"type":"code_mode_response","value":mbtx}),
        json!({"response_item":{"type":"code_mode_response","value":mbtx}}),
        json!({"response_item":{"type":"function_call_output","output":mbtx.to_string()}}),
    ] {
        assert_eq!(materialized_response_output(&result), mbtx);
    }
    assert_eq!(
        materialized_response_output(&json!({
            "response_item": {"value": {"output": {"ok": true}}}
        })),
        Value::Null
    );
}
