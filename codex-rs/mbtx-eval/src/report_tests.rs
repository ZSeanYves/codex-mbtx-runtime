use super::*;
use pretty_assertions::assert_eq;

#[test]
fn response_completion_requires_an_observed_terminal_event() {
    assert_eq!(
        response_terminal("data: {\"type\":\"response.created\"}\n\n"),
        None
    );
    assert_eq!(
        response_terminal(
            "event: response.completed\r\ndata: {\"type\":\r\ndata: \"response.completed\"}\r\n\r\n"
        ),
        Some("completed")
    );
    assert_eq!(
        response_terminal("data: {\"type\":\"error\"}\n\n"),
        Some("failed")
    );
}

#[test]
fn html_escapes_payloads_and_unknown_values_remain_visible() -> Result<()> {
    let model = json!({"run_id":"<script>run</script>","mode":"replay","platform":"test","partial":true,"itt":{},"conditional":{"pairs":0,"mean_step_difference":null},"attempts":[{"task_id":"<img src=x onerror=alert(1)>","arm":"mbtx_program","status":"censored","oracle":{"success":null},"accounting":{"metrics":{"agent_steps":null,"model_requests":null}},"evidence":{"directory":"attempts/id"}}]});
    let html = render(&model, "html")?;
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img"));
    assert!(html.contains("&lt;img"));
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
