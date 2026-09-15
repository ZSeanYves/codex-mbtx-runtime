//! One-shot capture reusing Codex spawning and sandboxing. Signal results are
//! actual wait observations, never synthetic timeout or cancellation codes.

use std::io;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::time::Duration;
use std::time::Instant;

use codex_tools::ToolProcessOutput;
use codex_tools::ToolProcessStatus;
use codex_utils_pty::process_group::kill_process_group;
use codex_utils_pty::process_group::terminate_process_group;
use tokio::process::Child;
use tokio_util::task::AbortOnDropHandle;

use super::ExecCapturePolicy;
use super::ExecExpiration;
use super::ExecExpirationOutcome;
use super::IO_DRAIN_TIMEOUT_MS;
use super::RawExecToolCallOutput;
use super::aggregate_output;
use super::execute_exec_request_raw;
use super::read_output;
use crate::sandboxing::ExecRequest;
use codex_protocol::error::Result;

pub(crate) async fn execute_bounded_request(
    mut request: ExecRequest,
    max_bytes: usize,
) -> Result<ToolProcessOutput> {
    request.capture_policy = ExecCapturePolicy::BoundedProcess { max_bytes };
    let started = Instant::now();
    let raw = execute_exec_request_raw(
        request, /*stdout_stream*/ None, /*after_spawn*/ None,
    )
    .await?;
    let duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    #[cfg(unix)]
    let signal = raw.exit_status.signal();
    #[cfg(not(unix))]
    let signal = None;
    let stdout_len = raw.stdout.text.len().min(max_bytes);
    let stderr_len = raw
        .stderr
        .text
        .len()
        .min(max_bytes.saturating_sub(stdout_len));
    Ok(ToolProcessOutput {
        status: match raw.termination {
            Some(ExecExpirationOutcome::TimedOut) => ToolProcessStatus::TimedOut,
            Some(ExecExpirationOutcome::Cancelled) => ToolProcessStatus::Cancelled,
            None => ToolProcessStatus::Exited,
        },
        exit_code: raw.exit_status.code(),
        signal,
        stdout: String::from_utf8_lossy(&raw.stdout.text[..stdout_len]).into_owned(),
        stderr: String::from_utf8_lossy(&raw.stderr.text[..stderr_len]).into_owned(),
        stdout_truncated: raw.stdout.text.len() > stdout_len,
        stderr_truncated: raw.stderr.text.len() > stderr_len,
        duration_ms,
    })
}

// Covers forced dispatch failure too. spawn_child_async separately arranges
// kill-on-drop/reaping of the leader; only our own group is signalled here.
struct OwnedGroup(Option<u32>);

impl Drop for OwnedGroup {
    fn drop(&mut self) {
        if let Some(pgid) = self.0 {
            let _ = kill_process_group(pgid);
        }
    }
}

pub(super) async fn consume(
    mut child: Child,
    expiration: ExecExpiration,
    max_bytes: usize,
) -> Result<RawExecToolCallOutput> {
    let mut group = OwnedGroup(child.id());
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing stderr pipe"))?;
    let cap = Some(max_bytes.saturating_add(1));
    let stdout = AbortOnDropHandle::new(tokio::spawn(read_output(
        stdout, /*stream*/ None, /*is_stderr*/ false, cap,
    )));
    let stderr = AbortOnDropHandle::new(tokio::spawn(read_output(
        stderr, /*stream*/ None, /*is_stderr*/ true, cap,
    )));

    let (exit_status, termination) = tokio::select! {
        biased;
        reason = expiration.wait_with_outcome() => {
            if let Some(pgid) = group.0 {
                terminate_process_group(pgid)?;
            }
            let status = match tokio::time::timeout(Duration::from_millis(500), child.wait()).await {
                Ok(status) => status?,
                Err(_) => {
                    if let Some(pgid) = group.0 {
                        kill_process_group(pgid)?;
                    }
                    child.start_kill()?;
                    tokio::time::timeout(Duration::from_secs(2), child.wait()).await
                        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "child reap deadline exceeded"))??
                }
            };
            (status, Some(reason))
        }
        status = child.wait() => (status?, None),
    };
    // A one-shot tool cannot leave background work running after its leader.
    // Descendants that deliberately escape the group are outside this contract.
    if let Some(pgid) = group.0 {
        kill_process_group(pgid)?;
    }
    group.0 = None;
    let drain = async {
        let (stdout, stderr) = tokio::join!(stdout, stderr);
        Ok::<_, io::Error>((
            stdout.map_err(io::Error::other)??,
            stderr.map_err(io::Error::other)??,
        ))
    };
    let (stdout, stderr) = tokio::time::timeout(Duration::from_millis(IO_DRAIN_TIMEOUT_MS), drain)
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "output pipes did not reach EOF after process-group cleanup",
            )
        })??;
    let aggregated_output = aggregate_output(&stdout, &stderr, cap);
    Ok(RawExecToolCallOutput {
        exit_status,
        stdout,
        stderr,
        aggregated_output,
        timed_out: termination == Some(ExecExpirationOutcome::TimedOut),
        termination,
    })
}

#[cfg(all(test, unix))]
#[path = "bounded_tests.rs"]
mod tests;
