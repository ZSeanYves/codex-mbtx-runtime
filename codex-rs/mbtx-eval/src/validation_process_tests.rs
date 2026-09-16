use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn preserves_nonzero_exit_stream_bytes_and_eof() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut command = Command::new("sh");
    command.args(["-c", "printf 'stdout 雪'; printf 'stderr' >&2; exit 7"]);
    let facts = capture(
        &mut command,
        &root.path().join("evidence"),
        Duration::from_secs(5),
    )
    .await?;
    assert_eq!(facts["exit_code"], 7);
    assert_eq!(facts["signal"], Value::Null);
    assert_eq!(facts["drain_complete"], true);
    assert_eq!(
        std::fs::read(root.path().join("evidence/stdout"))?,
        "stdout 雪".as_bytes()
    );
    assert_eq!(
        facts["streams"]["stdout"],
        json!({"bytes":10,"retained_bytes":10,"truncated":false,"eof":true})
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_reaps_owned_process_and_drains_its_inherited_pipes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30 & wait"]);
    let facts = capture(
        &mut command,
        &root.path().join("evidence"),
        Duration::from_millis(100),
    )
    .await?;
    assert_eq!(facts["timed_out"], true);
    assert_eq!(facts["wait_observed"], true);
    assert_eq!(facts["signal"], 9);
    assert_eq!(facts["exit_code"], Value::Null);
    assert_eq!(facts["drain_complete"], true);
    assert_eq!(facts["descendant_reap"], Value::Null);
    Ok(())
}
