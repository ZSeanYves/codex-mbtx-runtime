//! Per-thread compilation state. Cached Wasm bytes live in host memory, never
//! in task-writable files. Each execution gets its own fresh artifact copy.
use codex_tools::ToolProcessOutput;
use codex_utils_path_uri::PathUri;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Default)]
pub(crate) struct SessionCache(pub Mutex<CacheState>);

#[derive(Default)]
pub(crate) struct CacheState {
    pub prepared: Option<Prepared>,
    pub fingerprints: HashMap<std::path::PathBuf, (Stamp, String)>,
}

#[derive(PartialEq, Eq)]
pub(crate) struct Stamp {
    bytes: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    changed: (i64, i64),
}

/// Rehash changed inputs; unchanged, host-pinned dependencies only need stat.
/// File paths and content hashes both participate in the generation key.
pub(crate) fn inputs(
    roots: &[std::path::PathBuf],
    memo: &mut HashMap<std::path::PathBuf, (Stamp, String)>,
) -> std::io::Result<String> {
    use sha2::Digest;
    use sha2::Sha256;
    let mut pending = roots.to_vec();
    let mut files = Vec::new();
    let mut visited = std::collections::HashSet::new();
    while let Some(path) = pending.pop() {
        let canonical = std::fs::canonicalize(&path)?;
        if !visited.insert(canonical) {
            continue;
        }
        let metadata = std::fs::metadata(&path)?;
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_name() != "_build" && entry.file_name() != ".git" {
                    pending.push(entry.path());
                }
            }
        } else if metadata.is_file() {
            files.push((path, metadata));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut digest = Sha256::new();
    for (path, metadata) in files {
        let stamp = Stamp {
            bytes: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            changed: {
                use std::os::unix::fs::MetadataExt;
                (metadata.ctime(), metadata.ctime_nsec())
            },
        };
        if !memo.get(&path).is_some_and(|(old, _)| *old == stamp) {
            memo.insert(
                path.clone(),
                (
                    stamp,
                    format!("{:x}", Sha256::digest(std::fs::read(&path)?)),
                ),
            );
        }
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(memo[&path].1.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) struct Prepared {
    pub directory: PathUri,
    pub entries: HashMap<String, Compiled>,
}

#[derive(Clone)]
pub(crate) struct Compiled {
    pub bytes: Vec<u8>,
    pub build: ToolProcessOutput,
    pub call_id: String,
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
