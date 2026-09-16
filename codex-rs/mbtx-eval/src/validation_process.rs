//! Bounded OS capture for post-submission validation, outside model-step timing.
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

async fn drain(mut stream: impl AsyncRead + Unpin) -> Result<(Vec<u8>, u64)> {
    let mut bytes = Vec::new();
    let mut count = 0;
    let mut buffer = [0; 8192];
    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok((bytes, count));
        }
        count += n as u64;
        let keep = n.min(1_000_000usize.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
    }
}

pub(crate) async fn capture(
    command: &mut Command,
    directory: &Path,
    timeout: Duration,
) -> Result<Value> {
    std::fs::create_dir(directory)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .context("start sandboxed submission validation")?;
    let pid = child.id().context("validation process PID")?;
    let stdout = tokio::spawn(drain(child.stdout.take().context("stdout")?));
    let stderr = tokio::spawn(drain(child.stderr.take().context("stderr")?));
    let timed_out;
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => {
            timed_out = false;
            status?
        }
        Err(_) => {
            timed_out = true;
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            child.start_kill()?;
            child.wait().await?
        }
    };
    // Group cleanup is observed separately; it never establishes descendant reap.
    #[cfg(unix)]
    let residual_group = if unsafe { libc::kill(-(pid as i32), 0) } == 0 {
        Some(true)
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Some(false)
    } else {
        None
    };
    #[cfg(not(unix))]
    let residual_group: Option<bool> = None;
    #[cfg(unix)]
    if residual_group == Some(true) {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let mut streams = serde_json::Map::new();
    let mut complete = true;
    for (name, mut handle) in [("stdout", stdout), ("stderr", stderr)] {
        match tokio::time::timeout(Duration::from_secs(2), &mut handle).await {
            Ok(result) => {
                let (bytes, count) = result??;
                crate::evidence::write_new(&directory.join(name), &bytes)?;
                streams.insert(name.into(),json!({"bytes":count,"retained_bytes":bytes.len(),"truncated":count>bytes.len() as u64,"eof":true}));
            }
            Err(_) => {
                handle.abort();
                complete = false;
                streams.insert(
                    name.into(),
                    json!({"bytes":null,"retained_bytes":null,"truncated":null,"eof":false}),
                );
            }
        }
    }
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    let value = json!({"exit_code":status.code(),"signal":signal,"timed_out":timed_out,"wait_observed":true,"drain_complete":complete,"residual_group_before_cleanup":residual_group,"descendant_reap":null,"elapsed_ns":started.elapsed().as_nanos() as u64,"streams":streams});
    crate::evidence::json_new(&directory.join("process.json"), &value)?;
    Ok(value)
}

#[cfg(test)]
#[path = "validation_process_tests.rs"]
mod tests;
