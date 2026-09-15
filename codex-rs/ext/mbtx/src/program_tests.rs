use super::ProgramResult;

use codex_tools::ToolProcessOutput;
use codex_tools::ToolProcessStatus;
use pretty_assertions::assert_eq;

#[test]
fn escaped_output_fits_the_response_budget_without_fabricating_observations() {
    let mut result = ProgramResult {
        status: "error",
        stage: "completed",
        target: "wasm",
        source_path: None,
        artifact_path: None,
        build: None,
        error: None,
        run: Some(ToolProcessOutput {
            status: ToolProcessStatus::Cancelled,
            exit_code: None,
            signal: None,
            stdout: "\0雪\\\n".repeat(600),
            stderr: "\0".repeat(4096),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 12,
        }),
    };
    result.fit_response(1024).expect("bounded result");
    let json = serde_json::to_value(&result).expect("JSON");
    assert!(serde_json::to_vec(&result).expect("serialized JSON").len() <= 1024);
    assert_eq!(
        (
            json["run"]["exit_code"].clone(),
            json["run"]["signal"].clone(),
            json["run"]["stdout_truncated"].clone(),
            json["run"]["stderr_truncated"].clone()
        ),
        (
            serde_json::Value::Null,
            serde_json::Value::Null,
            true.into(),
            true.into()
        )
    );
}
