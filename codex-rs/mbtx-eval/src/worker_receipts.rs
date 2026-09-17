//! Attempt-scoped receipts from the real, hash-verified fixture executable.
//! The socket has no oracle API and cannot launch processes or read task files.
#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::net::UnixStream;

pub(crate) struct WorkerReceipts {
    pub socket: PathBuf,
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<Result<()>>,
}

fn peer_executable(stream: &UnixStream) -> Result<(i32, PathBuf)> {
    #[cfg(target_os = "linux")]
    {
        let pid = stream
            .peer_cred()?
            .pid()
            .context("worker peer PID missing")?;
        Ok((pid, PathBuf::from(format!("/proc/{pid}/exe"))))
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let mut pid: libc::pid_t = 0;
        let mut length = std::mem::size_of_val(&pid) as libc::socklen_t;
        ensure!(
            unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    0,
                    libc::LOCAL_PEERPID,
                    (&mut pid as *mut libc::pid_t).cast(),
                    &mut length,
                )
            } == 0,
            "cannot observe worker peer PID"
        );
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let size = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        ensure!(size > 0, "cannot observe worker executable");
        let end = path.iter().position(|b| *b == 0).unwrap_or(size as usize);
        Ok((pid, PathBuf::from(std::str::from_utf8(&path[..end])?)))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = stream;
        anyhow::bail!("worker identity requires Linux or macOS")
    }
}

impl WorkerReceipts {
    pub async fn start(evidence: &Path, executable: &Path) -> Result<Self> {
        // Unix socket paths have a small OS limit; never put them under a long run path.
        let parent = Path::new("/tmp").join(format!("mw-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir(&parent)?;
        let socket = parent.join("s");
        let listener = UnixListener::bind(&socket)?;
        let hash = crate::evidence::digest(&fs::read(executable)?);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(evidence.join("worker-events.jsonl"))?;
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let clock = Instant::now();
            let clock_domain = format!("worker-receipts-{}", uuid::Uuid::new_v4());
            let mut sequence = 0u64;
            loop {
                let (stream, _) = tokio::select! { result = listener.accept() => result?, _ = &mut stopped => break };
                let received_ns = clock.elapsed().as_nanos() as u64;
                let result: Result<(i32, Value)> = async {
                    let (pid, executable) = peer_executable(&stream)?;
                    ensure!(crate::evidence::digest(&fs::read(&executable)?) == hash, "unregistered worker executable");
                    let mut reader = BufReader::new(stream.take(65537));
                    let mut text = String::new();
                    tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_line(&mut text)).await??;
                    ensure!(text.len() <= 65536, "worker event exceeds bound");
                    let event: Value = serde_json::from_str(&text)?;
                    let record = json!({"sequence":sequence,"pid":pid,"received_ns":received_ns,"clock_domain":clock_domain,"executable_sha256":hash,"event":event,"confidence":"observed"});
                    serde_json::to_writer(&mut file, &record)?;
                    file.write_all(b"\n")?;
                    reader.get_mut().get_mut().write_all(b"recorded\n").await?;
                    Ok((pid, record))
                }.await;
                if let Err(error) = result {
                    serde_json::to_writer(
                        &mut file,
                        &json!({"sequence":sequence,"received_ns":received_ns,"evidence_error":error.to_string()}),
                    )?;
                    file.write_all(b"\n")?;
                }
                sequence += 1;
            }
            file.sync_all()?;
            Ok(())
        });
        Ok(Self { socket, stop, task })
    }

    pub async fn finish(self) -> Result<()> {
        let _ = self.stop.send(());
        let result = self.task.await?;
        let _ = fs::remove_file(&self.socket);
        if let Some(parent) = self.socket.parent() {
            let _ = fs::remove_dir(parent);
        }
        result
    }
}
