use super::*;
use pretty_assertions::assert_eq;

#[test]
fn task_rules_allow_literal_queries_and_reject_generic_execution() -> Result<()> {
    let task = json!({"process_allow":[
        {"program":"jq","args_prefix":[]},
        {"program":"rg","args_prefix":["--no-config","--json","--"]},
        {"program":"fixture-worker","args_prefix":["job"]}
    ]});
    assert_eq!(rules(&task)?, task["process_allow"].as_array());
    for rule in [
        json!({"program":"sh","args_prefix":["-c"]}),
        json!({"program":"/bin/sh","args_prefix":[]}),
        json!({"program":"rg","args_prefix":[]}),
        json!({"program":"fixture-worker","args_prefix":["launch"]}),
    ] {
        assert!(rules(&json!({"process_allow":[rule]})).is_err());
    }
    assert!(rules(&json!({})).unwrap().is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn prepare_freezes_host_path_and_rejects_changed_utility() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let work = root.join("work");
    let evidence = root.join("evidence");
    fs::create_dir(&evidence)?;
    for child in ["workspace", "tmp"] {
        fs::create_dir_all(work.join(child))?;
    }
    crate::evidence::json_new(
        &work.join("worker-socket.json"),
        &json!("/tmp/receipt.sock"),
    )?;
    let executable = which::which("true")?.canonicalize()?;
    #[cfg(target_os = "linux")]
    {
        fs::create_dir(root.join("codex-resources"))?;
        fs::copy(&executable, root.join("codex-resources/bwrap"))?;
    }
    let utilities =
        json!({"jq":{"path":executable,"sha256":crate::evidence::digest(&fs::read(&executable)?)}});
    let task = json!({"process_allow":[{"program":"jq","args_prefix":[]}]});
    let policy = prepare(&task, &work, &evidence, &root, &utilities)?.context("policy")?;
    let value = crate::evidence::read_json(&policy.path)?;
    assert_eq!(value["process"], json!({"allow":task["process_allow"]}));
    assert_eq!(value["env"]["set"]["PATH"], policy.execution_path);
    assert_eq!(fs::canonicalize(work.join("tools/jq"))?, executable);
    assert_eq!(fs::metadata(&policy.path)?.permissions().mode() & 0o222, 0);
    assert_eq!(
        policy.sha256,
        crate::evidence::digest(&fs::read(&policy.path)?)
    );
    fs::set_permissions(work.join("tools"), fs::Permissions::from_mode(0o755))?;
    fs::remove_file(work.join("tools/jq"))?;
    #[cfg(target_os = "linux")]
    fs::remove_file(work.join("tools/bwrap"))?;
    fs::remove_dir(work.join("tools"))?;
    let changed = json!({"jq":{"path":executable,"sha256":"changed"}});
    assert!(prepare(&task, &work, &evidence, &root, &changed).is_err());
    Ok(())
}
