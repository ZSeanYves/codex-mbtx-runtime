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
        cache: "cold",
        build_reused_from: None,
        preparation_ms: 0,
        source_resource: None,
        artifact_resource: None,
        run: Some(ToolProcessOutput {
            status: ToolProcessStatus::Cancelled,
            exit_code: None,
            signal: None,
            stdout: "\0雪\\\n".repeat(600),
            stderr: "\0".repeat(4096),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 12,
            resources: vec![],
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

#[test]
fn successful_compiler_warnings_do_not_evict_runtime_output() {
    let output = |stdout: String, stderr: String| ToolProcessOutput {
        status: ToolProcessStatus::Exited,
        exit_code: Some(0),
        signal: None,
        stdout,
        stderr,
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 1,
        resources: vec![],
    };
    let mut result = ProgramResult {
        status: "success",
        stage: "completed",
        target: "wasm",
        source_path: None,
        artifact_path: None,
        error: None,
        cache: "cold",
        build_reused_from: None,
        preparation_ms: 0,
        source_resource: None,
        artifact_resource: None,
        build: Some(output(String::new(), "compiler warning 雪\n".repeat(1000))),
        run: Some(output("observed runtime output\n".into(), String::new())),
    };
    result.fit_response(700).unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() <= 700);
    assert!(result.build.unwrap().stderr_truncated);
    let run = result.run.unwrap();
    assert_eq!(
        (run.stdout, run.stdout_truncated),
        ("observed runtime output\n".into(), false)
    );
}
