use super::*;
use pretty_assertions::assert_eq;

#[test]
fn response_completion_requires_an_observed_terminal_event() {
    assert_eq!(
        response_summary("data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-1\"}}\n\n"),
        (None, Some("response-1".into()))
    );
    assert_eq!(
        response_summary(
            "event: response.completed\r\ndata: {\"type\":\r\ndata: \"response.completed\"}\r\n\r\n"
        ),
        (Some("completed"), None)
    );
    assert_eq!(
        response_summary("data: {\"type\":\"error\"}\n\n"),
        (Some("failed"), None)
    );
}

#[test]
fn html_escapes_payloads_and_unknown_values_remain_visible() -> Result<()> {
    let model = json!({"run_id":"<script>run</script>","mode":"replay","platform":"test","partial":true,"itt":{},"conditional":{"pairs":0,"mean_step_difference":null},"attempts":[{"task_id":"<img src=x onerror=alert(1)>","arm":"mbtx_program","status":"censored","oracle":{"success":null},"accounting":{"metrics":{"agent_steps":null,"model_requests":null}},"evidence":{"directory":"attempts/id"}}]});
    let html = render(&model, "html")?;
    assert!(!html.contains("<script>run</script>"));
    assert!(!html.contains("<img src=x onerror=alert(1)>"));
    assert!(html.contains("\\u003cimg"));
    assert!(html.contains("null"));
    let csv = render(&model, "csv")?;
    assert!(csv.contains("\"null\""));
    Ok(())
}

#[test]
fn attempt_creation_and_seal_refuse_overwrite_and_detect_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("evidence.json");
    crate::evidence::write_new(&file, b"original")?;
    assert!(crate::evidence::write_new(&file, b"replace").is_err());
    crate::evidence::seal(root.path())?;
    assert!(verify(root.path())?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(&file)?.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&file, permissions)?;
    }
    fs::write(file, b"changed")?;
    assert!(!verify(root.path())?);
    Ok(())
}

#[test]
fn snapshot_rejects_symlink_escape_and_missing_outputs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace)?;
    fs::write(root.path().join("private"), "private")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.path().join("private"), workspace.join("result.json"))?;
    let snapshot =
        crate::evidence::snapshot(&workspace, &json!({"files":{},"output":"result.json"}))?;
    #[cfg(unix)]
    assert_eq!(snapshot["errors"], json!(["result.json"]));
    assert_eq!(snapshot["files"], json!({}));
    Ok(())
}

#[test]
fn html_embeds_every_step_and_original_bytes_without_executable_payload_text() -> Result<()> {
    use base64::Engine;
    use std::io::Read;
    let steps:Vec<_>=(0..512).map(|n|json!({"step_id":format!("step-{n}"),"outcome":"accepted","source":"</script><script>globalThis.injected=true</script>雪"})).collect();
    let attempt=json!({"attempt_id":"large","accounting":{"steps":steps,"metrics":{"agent_steps":512}},"details":{"resources":[{"data_base64":"AP8="}]}});
    let model=json!({"run_id":"</script><script>bad()</script>","attempts":[attempt]});
    let html=render(&model,"html")?;
    assert!(!html.contains("<script>bad()"));
    assert!(!html.contains("<script>globalThis.injected"));
    let marker="id=\"attempt-0\">";
    let encoded=html.split_once(marker).unwrap().1.split_once("</script>").unwrap().0;
    let bytes=base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let mut decoder=flate2::read::GzDecoder::new(bytes.as_slice());
    let mut decoded=String::new();decoder.read_to_string(&mut decoded)?;
    assert_eq!(serde_json::from_str::<Value>(&decoded)?,attempt);
    assert!(html.contains("connect-src 'none'"));
    Ok(())
}

#[test]
fn partially_written_resource_metadata_cannot_prevent_report_reconstruction() -> Result<()> {
    let root=tempfile::tempdir()?;
    fs::create_dir(root.path().join("resources"))?;
    fs::write(root.path().join("resources/interrupted.json"),b"{\"resource_id\":")?;
    let details=crate::report_details::collect(root.path())?;
    assert_eq!(details["resources"],json!([]));
    assert_eq!(details["errors"].as_array().unwrap().len(),1);
    Ok(())
}

#[test]
fn offline_report_navigation_and_initial_data_snapshot() -> Result<()> {
    let model = json!({"run_id":"review-example","mode":"replay","partial":true,"attempts":[{"attempt_id":"one","status":"censored","accounting":{"steps":[{"step_id":"s1"}],"metrics":{"agent_steps_started":1,"agent_steps":null},"coverage":{"steps":false},"complete":false},"details":{"resources":[]}}]});
    let html = render(&model, "html")?;
    // Snapshot the rendered navigation and initial data. Compressed evidence and
    // vendored JavaScript are covered by the round-trip test, not UI snapshots.
    let body = html.split_once("<body>").unwrap().1.split_once("<script type=\"application/octet-stream\"").unwrap().0;
    insta::assert_snapshot!(body);
    Ok(())
}
