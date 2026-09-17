use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn sandbox_start_failure_is_sealed_without_starting_task_attempts() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let bundle = root.join("bundle");
    fs::create_dir(&bundle)?;
    // An actual failing executable models sandbox startup rejection. No fake
    // model responses, provider credentials, or test-specific bypass is needed.
    std::os::unix::fs::symlink(which::which("false")?, bundle.join("codex"))?;
    fs::copy(which::which("true")?, bundle.join("fixture-worker"))?;
    let info = json!({"moon_home":root});
    let path = std::env::var("PATH")?;
    let error = check(
        &root,
        &Validator {
            bundle: &bundle,
            bundle_info: &info,
            path: &path,
        },
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("before API requests"));
    assert!(!root.join("attempts").exists());
    let evidence = fs::read_dir(&root)?
        .filter_map(std::result::Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("preflight-")
        })
        .context("preflight evidence")?
        .path();
    assert!(crate::evidence::verify(&evidence)?);
    let result = crate::evidence::read_json(&evidence.join("result.json"))?;
    assert_eq!(result["status"], "harness_error");
    assert_eq!(crate::evidence::worker_events(&evidence)?, json!([]));
    let process = crate::evidence::read_json(&evidence.join("execution/process.json"))?;
    assert_eq!(process["exit_code"], 1);
    assert_eq!(process["timed_out"], false);
    assert_eq!(process["drain_complete"], true);
    Ok(())
}
