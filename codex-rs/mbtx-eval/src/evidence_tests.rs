use super::*;
use pretty_assertions::assert_eq;

#[test]
fn snapshot_captures_the_declared_worker_plan_independently_of_expected_output_order() -> Result<()>
{
    let work = tempfile::tempdir()?;
    fs::write(work.path().join("result.json"), b"{\"total\":3}")?;
    fs::write(
        work.path().join("jobs-selected.json"),
        b"{\"jobs\":[{\"id\":\"second\"},{\"id\":\"first\"}]}",
    )?;
    let task = json!({
        "files":{},"output":"result.json","expected_outputs":{},
        "worker_plan_path":"jobs-selected.json"
    });
    assert_eq!(
        snapshot(work.path(), &task)?,
        json!({
            "files":{
                "result.json":"{\"total\":3}",
                "jobs-selected.json":"{\"jobs\":[{\"id\":\"second\"},{\"id\":\"first\"}]}"
            },"errors":[]
        })
    );
    fs::remove_file(work.path().join("jobs-selected.json"))?;
    assert_eq!(
        snapshot(work.path(), &task)?,
        json!({
            "files":{"result.json":"{\"total\":3}"},"errors":[]
        })
    );
    assert!(
        snapshot(
            work.path(),
            &json!({
                "files":{},"output":"result.json","worker_plan_path":"../outside"
            })
        )
        .is_err()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn snapshot_records_a_declared_worker_plan_symlink_as_incomplete_evidence() -> Result<()> {
    let work = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("plan.json"), b"{\"jobs\":[]}")?;
    std::os::unix::fs::symlink(
        outside.path().join("plan.json"),
        work.path().join("plan.json"),
    )?;
    assert_eq!(
        snapshot(
            work.path(),
            &json!({
                "files":{},"output":"result.json","worker_plan_path":"plan.json"
            })
        )?,
        json!({"files":{},"errors":["plan.json"]})
    );
    Ok(())
}
