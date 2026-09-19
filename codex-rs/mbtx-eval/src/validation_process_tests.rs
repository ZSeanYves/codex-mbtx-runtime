use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn exhausted_budget_never_spawns_a_process() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut command = Command::new("this-command-must-not-be-spawned");
    let facts = capture(&mut command, &root.path().join("evidence"), Duration::ZERO).await?;
    assert_eq!(
        facts,
        json!({"started":false,"exit_code":null,"signal":null,"timed_out":true,"cancelled":false,"wait_observed":false,"drain_complete":true,"residual_group_before_cleanup":null,"descendant_reap":null,"elapsed_ns":0,"streams":{}})
    );
    Ok(())
}

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

#[cfg(unix)]
#[tokio::test]
async fn collector_interrupt_cancels_validation_without_reporting_a_deadline() -> Result<()> {
    // nextest isolates each test in its own process. The child signals this
    // capture's collector, exercising the same SIGINT path as the evaluator.
    let root = tempfile::tempdir()?;
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 0.1; kill -INT \"$PPID\"; sleep 30"]);
    let facts = capture(
        &mut command,
        &root.path().join("evidence"),
        Duration::from_secs(5),
    )
    .await?;
    assert_eq!(
        json!({"cancelled":facts["cancelled"],"timed_out":facts["timed_out"],"wait":facts["wait_observed"],"drain":facts["drain_complete"]}),
        json!({"cancelled":true,"timed_out":false,"wait":true,"drain":true})
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn interruption_between_phases_prevents_the_next_process_start() -> Result<()> {
    let cancellation = crate::cancellation::Cancellation::listen()?;
    unsafe { libc::raise(libc::SIGINT) };
    tokio::time::timeout(Duration::from_secs(1), cancellation.cancelled()).await?;
    let root = tempfile::tempdir()?;
    let mut command = Command::new("must-not-start-after-interruption");
    let facts = capture_cancellable(
        &mut command,
        &root.path().join("evidence"),
        Duration::from_secs(10),
        &cancellation,
    )
    .await?;
    assert_eq!(
        facts,
        json!({"started":false,"exit_code":null,"signal":null,"timed_out":false,"cancelled":true,"wait_observed":false,"drain_complete":true,"residual_group_before_cleanup":null,"descendant_reap":null,"elapsed_ns":0,"streams":{}})
    );
    Ok(())
}
