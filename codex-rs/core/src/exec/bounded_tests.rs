use super::*;

use std::process::Stdio;

use pretty_assertions::assert_eq;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

fn command(source: &str) -> io::Result<Child> {
    Command::new("/bin/sh")
        .args(["-c", source])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

#[tokio::test]
async fn numeric_exits_and_real_signals_remain_distinct() {
    for (source, code, signal) in [
        ("exit 143", Some(143), None),
        ("kill -TERM $$", None, Some(15)),
        ("kill -KILL $$", None, Some(9)),
    ] {
        let raw = consume(
            command(source).expect("spawn"),
            ExecExpiration::from(2000),
            64,
        )
        .await
        .expect("capture");
        assert_eq!(
            (
                raw.exit_status.code(),
                raw.exit_status.signal(),
                raw.termination
            ),
            (code, signal, None)
        );
    }
}

#[tokio::test]
async fn cancellation_reaps_the_leader_and_drains_both_streams() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let ready = directory.path().join("ready");
    let source = format!(
        "printf out; printf err >&2; touch {}; sleep 30",
        shlex::try_quote(ready.to_str().expect("path")).expect("quote")
    );
    let child = command(&source).expect("spawn");
    let pid = child.id().expect("leader PID");
    let cancellation = CancellationToken::new();
    let capture = tokio::spawn(consume(
        child,
        ExecExpiration::Cancellation(cancellation.clone()),
        64,
    ));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child readiness");
    cancellation.cancel();
    let raw = capture.await.expect("capture task").expect("capture");
    assert_eq!(
        (raw.termination, raw.stdout.text, raw.stderr.text),
        (
            Some(ExecExpirationOutcome::Cancelled),
            b"out".to_vec(),
            b"err".to_vec()
        )
    );
    // A second wait cannot reap the already collected direct child.
    let waited = unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(
        (waited, io::Error::last_os_error().raw_os_error()),
        (-1, Some(libc::ECHILD))
    );
}

#[tokio::test]
async fn ignored_term_escalates_and_reports_sigkill() {
    let raw = consume(
        command("trap '' TERM; while :; do sleep 1; done").expect("spawn"),
        ExecExpiration::from(200),
        64,
    )
    .await
    .expect("capture");
    assert_eq!(
        (
            raw.termination,
            raw.exit_status.code(),
            raw.exit_status.signal()
        ),
        (Some(ExecExpirationOutcome::TimedOut), None, Some(9))
    );
}

#[tokio::test]
async fn inherited_pipes_are_cleaned_after_the_leader_exits() {
    let raw = tokio::time::timeout(
        Duration::from_secs(3),
        consume(
            command("sleep 30 & printf tail; printf stderr-tail >&2").expect("spawn"),
            ExecExpiration::from(10_000),
            64,
        ),
    )
    .await
    .expect("drain completed")
    .expect("capture");
    assert_eq!(
        (raw.exit_status.code(), raw.stdout.text, raw.stderr.text),
        (Some(0), b"tail".to_vec(), b"stderr-tail".to_vec())
    );
}

#[tokio::test]
async fn bounded_capture_keeps_overflow_marker_and_continues_to_eof() {
    let raw = consume(
        command("i=0; while [ $i -lt 10000 ]; do printf x; printf y >&2; i=$((i+1)); done")
            .expect("spawn"),
        ExecExpiration::from(5000),
        32,
    )
    .await
    .expect("capture");
    assert_eq!(
        (raw.exit_status.code(), raw.stdout.text, raw.stderr.text),
        (Some(0), vec![b'x'; 33], vec![b'y'; 33])
    );
}
