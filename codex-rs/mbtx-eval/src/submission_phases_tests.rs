use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn phase_updates_preserve_state_and_require_declared_input_bytes() -> Result<()> {
    let work = tempfile::tempdir()?;
    fs::write(work.path().join("state.json"), b"committed")?;
    let phase = json!({"files":{"input.json":"new generation"},"input_updates":{"input.json":"new generation"}});
    update_inputs(work.path(), &phase)?;
    assert_eq!(
        (
            fs::read(work.path().join("state.json"))?,
            fs::read(work.path().join("input.json"))?
        ),
        (b"committed".to_vec(), b"new generation".to_vec())
    );
    let mismatch =
        json!({"files":{"input.json":"other"},"input_updates":{"input.json":"new generation"}});
    assert!(update_inputs(work.path(), &mismatch).is_err());
    for path in [
        "../escape",
        "solution.mbtx",
        "solution.sh",
        ".git/config",
        "fixture-worker",
    ] {
        let value = json!({"files":{path:"forbidden"},"input_updates":{path:"forbidden"}});
        assert!(update_inputs(work.path(), &value).is_err(), "{path}");
    }
    Ok(())
}

#[test]
fn phase_plan_rejects_missing_initial_or_recursive_contract() -> Result<()> {
    let initial = json!({"files":{"input":"bytes"},"input_updates":{},"expected":{}});
    let case = json!({"files":{"input":"bytes"},"validation_phases":[initial.clone(),initial]});
    assert_eq!(phases(&case)?, vec![initial.clone(), initial]);
    assert!(phases(&json!({"files":{},"validation_phases":[]})).is_err());
    assert!(phases(&json!({"files":{},"validation_phases":[{"files":{},"expected":{},"input_updates":{},"validation_phases":[]}]})).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn phase_updates_cannot_follow_a_program_authored_symlink() -> Result<()> {
    let work = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    std::os::unix::fs::symlink(external.path(), work.path().join("inputs"))?;
    let phase = json!({"files":{"inputs/a":"x"},"input_updates":{"inputs/a":"x"}});
    assert!(update_inputs(work.path(), &phase).is_err());
    assert!(!external.path().join("a").exists());
    Ok(())
}

#[test]
fn preservation_hashes_distinguish_changed_deleted_and_missing_prior_state() -> Result<()> {
    let work = tempfile::tempdir()?;
    fs::write(work.path().join("cache.bin"), b"\0stable\r\n")?;
    fs::create_dir(work.path().join("directory"))?;
    let phase = json!({"preserve_previous":["cache.bin","missing.bin","directory"]});
    let initial = preserved_hashes(work.path(), &phase)?;
    assert_eq!(
        initial,
        json!({
            "cache.bin":crate::evidence::digest(b"\0stable\r\n"),
            "missing.bin":null,"directory":null
        })
    );
    fs::write(work.path().join("cache.bin"), b"\0changed\r\n")?;
    assert_eq!(
        preserved_hashes(work.path(), &phase)?,
        json!({
            "cache.bin":crate::evidence::digest(b"\0changed\r\n"),
            "missing.bin":null,"directory":null
        })
    );
    fs::remove_file(work.path().join("cache.bin"))?;
    assert_eq!(
        preserved_hashes(work.path(), &phase)?,
        json!({
            "cache.bin":null,"missing.bin":null,"directory":null
        })
    );
    assert_eq!(preserved_hashes(work.path(), &json!({}))?, json!({}));
    assert!(preserved_hashes(work.path(), &json!({"preserve_previous":["../outside"]})).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn preservation_does_not_hash_symlinks_or_a_parent_escape_as_owned_state() -> Result<()> {
    let work = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    fs::write(external.path().join("target"), b"outside")?;
    std::os::unix::fs::symlink(external.path().join("target"), work.path().join("direct"))?;
    std::os::unix::fs::symlink(external.path(), work.path().join("parent"))?;
    assert_eq!(
        preserved_hashes(
            work.path(),
            &json!({
                "preserve_previous":["direct","parent/target"]
            })
        )?,
        json!({"direct":null,"parent/target":null})
    );
    Ok(())
}

#[test]
fn phase_plan_rejects_incomplete_evidence_contract_before_execution() {
    let first = json!({"files":{"input":"old"},"input_updates":{},"expected":{}});
    for malformed in [
        json!({"files":{"input":"new"},"expected":{}}),
        json!({"files":{"input":"new"},"input_updates":{}}),
        json!({"expected":{},"input_updates":{}}),
    ] {
        assert!(
            phases(&json!({"files":{"input":"old"},"validation_phases":[first.clone(),malformed]}))
                .is_err()
        );
    }
    assert!(phases(&json!({"files":{"input":"different"},"validation_phases":[first]})).is_err());
    assert!(
        phases(&json!({"files":{},"validation_phases":[{
            "files":{},"expected":{},"input_updates":{"input":"new"}
        }]}))
        .is_err()
    );
}
