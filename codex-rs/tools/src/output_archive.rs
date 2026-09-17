//! Host-owned lossless output resources, independent of preview/context limits.
//! The root is selected by the host and must not be writable by task processes.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputResource {
    pub resource_id: String,
    pub call_id: String,
    pub phase: String,
    pub stream: String,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub eof: bool,
    pub complete: bool,
    pub error: Option<String>,
    pub write_ns: u64,
    pub started_unix_ms: u64,
    pub ended_unix_ms: Option<u64>,
}

struct StreamState {
    file: File,
    hash: Sha256,
    receipt: OutputResource,
    sealed: bool,
}

/// One real stream. Append records bytes before any preview truncation.
#[derive(Clone)]
pub struct OutputArchive {
    root: PathBuf,
    state: Arc<Mutex<StreamState>>,
}

fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

impl OutputArchive {
    pub fn create(root: &Path, call_id: &str, phase: &str, stream: &str) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let resource_id = Uuid::new_v4().to_string();
        let receipt = OutputResource {
            resource_id: resource_id.clone(), call_id: call_id.into(), phase: phase.into(),
            stream: stream.into(), bytes: 0, sha256: None, eof: false, complete: false,
            error: None, write_ns: 0, started_unix_ms: unix_ms(), ended_unix_ms: None,
        };
        let file = create_private(&root.join(format!("{resource_id}.data")))?;
        serde_json::to_writer(create_private(&root.join(format!("{resource_id}.json")))?, &receipt)?;
        Ok(Self { root: root.to_owned(), state: Arc::new(Mutex::new(StreamState {
            file, hash: Sha256::new(), receipt, sealed: false,
        })) })
    }

    pub fn append(&self, bytes: &[u8]) {
        let started = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.sealed { return; }
        state.receipt.bytes += bytes.len() as u64;
        state.hash.update(bytes);
        if state.receipt.error.is_none() && let Err(error) = state.file.write_all(bytes) {
            state.receipt.error = Some(error.to_string());
        }
        state.receipt.write_ns += started.elapsed().as_nanos() as u64;
    }

    /// Observation failures do not terminate or change the child process.
    pub fn record_error(&self, error: &str) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.sealed { state.receipt.error.get_or_insert_with(|| error.into()); }
    }

    pub fn finish(&self) -> OutputResource {
        self.close(/*eof*/ true)
    }

    pub fn receipt(&self) -> OutputResource {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).receipt.clone()
    }

    pub fn interrupted(&self) -> OutputResource {
        self.close(/*eof*/ false)
    }

    fn close(&self, eof: bool) -> OutputResource {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.sealed {
            let started = Instant::now();
            if let Err(error) = state.file.sync_data() {
                state.receipt.error.get_or_insert_with(|| error.to_string());
            }
            state.receipt.write_ns += started.elapsed().as_nanos() as u64;
            state.receipt.eof = eof;
            state.receipt.complete = eof && state.receipt.error.is_none();
            state.receipt.ended_unix_ms = Some(unix_ms());
            state.receipt.sha256 = state.receipt.error.is_none().then(|| format!("{:x}", state.hash.clone().finalize()));
            let id = &state.receipt.resource_id;
            let temporary = self.root.join(format!("{id}.partial"));
            let saved = (|| {
                serde_json::to_writer(create_private(&temporary)?, &state.receipt)?;
                fs::rename(&temporary, self.root.join(format!("{id}.json")))
            })();
            if let Err(error) = saved {
                state.receipt.complete = false;
                state.receipt.error = Some(error.to_string());
            }
            state.sealed = true;
        }
        state.receipt.clone()
    }
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct ResourcePage {
    pub resource_id: String,
    pub offset: u64,
    pub next_offset: u64,
    pub total_bytes: u64,
    pub eof: bool,
    pub text: String,
}

/// Read only an issued output ID. No caller-supplied filesystem paths are used.
pub fn read_output(root: &Path, id: &str, offset: u64, max_bytes: usize) -> io::Result<ResourcePage> {
    let id = Uuid::parse_str(id).map_err(io::Error::other)?.to_string();
    let metadata: OutputResource = serde_json::from_reader(File::open(root.join(format!("{id}.json")))?)?;
    if metadata.resource_id != id { return Err(io::Error::other("resource identity mismatch")); }
    read_text_page(&root.join(format!("{id}.data")), &id, offset, max_bytes)
}

/// Byte offsets are UTF-8 boundaries; invalid offsets are explicit errors.
pub fn read_text_page(path: &Path, id: &str, offset: u64, max_bytes: usize) -> io::Result<ResourcePage> {
    if !(4..=65536).contains(&max_bytes) { return Err(io::Error::other("max_bytes must be 4..65536")); }
    if fs::symlink_metadata(path)?.file_type().is_symlink() { return Err(io::Error::other("resource cannot be a symlink")); }
    let mut file = File::open(path)?;
    let total_bytes = file.metadata()?.len();
    if offset > total_bytes { return Err(io::Error::other("offset exceeds resource size")); }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64).read_to_end(&mut bytes)?;
    let end = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() && offset + (bytes.len() as u64) < total_bytes => error.valid_up_to(),
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
    };
    let next_offset = offset + end as u64;
    Ok(ResourcePage { resource_id: id.into(), offset, next_offset, total_bytes,
        eof: next_offset == total_bytes, text: String::from_utf8(bytes[..end].to_vec()).map_err(io::Error::other)? })
}

#[cfg(test)]
#[path = "output_archive_tests.rs"]
mod tests;
