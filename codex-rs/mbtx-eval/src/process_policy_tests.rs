use super::*;
use pretty_assertions::assert_eq;

#[test]
fn task_rules_allow_literal_queries_and_reject_generic_execution() -> Result<()> {
    let task = json!({
        "cohort":"natural-tool-choice",
        "process_policy_profile":"natural-direct-process-v1",
        "process_allow":[
        {"program":"jq","args_prefix":[]},
        {"program":"rg","args_prefix":["--no-config","--json","--"]},
        {"program":"fixture-worker","args_prefix":["job"]},
        {"program":"git","args_prefix":[]},
        {"program":"moon","args_prefix":[]}
    ]});
    assert_eq!(rules(&task)?, task["process_allow"].as_array());
    assert!(
        rules(&json!({"process_allow":[
            {"program":"rg","args_prefix":["--no-config","--json","--"]},
            {"program":"rg","args_prefix":["--no-config","--files","--"]}
        ]}))
        .is_ok()
    );
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

#[test]
fn generic_v4_tasks_cannot_admit_natural_tooling() {
    for program in ["git", "moon"] {
        let task = json!({
            "process_policy_profile":"v4-direct-process-v1",
            "process_allow":[{"program":program,"args_prefix":[]}]
        });
        assert!(rules(&task).is_err(), "{program} must remain profile-gated");
    }
    let mismatched = json!({
        "cohort":"natural-tool-choice",
        "process_policy_profile":"v4-direct-process-v1",
        "process_allow":[{"program":"git","args_prefix":[]}]
    });
    assert!(rules(&mismatched).is_err());
}

#[test]
fn policy_rules_reject_paths_duplicates_extra_fields_and_nul() {
    for process_allow in [
        json!([{"program":"./jq","args_prefix":[]}]),
        json!([
            {"program":"jq","args_prefix":[]},
            {"program":"jq","args_prefix":[]}
        ]),
        json!([{"program":"jq","args_prefix":[],"scope":"workspace"}]),
        json!([{"program":"rg","args_prefix":["--no-config\u{0000}","--json","--"]}]),
    ] {
        assert!(rules(&json!({"process_allow":process_allow})).is_err());
    }
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
    let task = json!({
        "process_policy_profile":"v4-direct-process-v1",
        "process_allow":[{"program":"jq","args_prefix":[]}]
    });
    let policy = prepare(&task, &work, &evidence, &root, &utilities)?.context("policy")?;
    let value = crate::evidence::read_json(&policy.path)?;
    assert_eq!(value["process"], json!({"allow":task["process_allow"]}));
    let evidence_value = crate::evidence::read_json(&evidence.join("policy-evidence.json"))?;
    assert_eq!(
        evidence_value["policy_profile"],
        task["process_policy_profile"]
    );
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
