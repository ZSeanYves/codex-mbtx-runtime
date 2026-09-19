use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use walkdir::WalkDir;

pub(crate) fn now_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

pub(crate) fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("exclusive evidence creation: {}", path.display()))?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn json_new(path: &Path, value: &Value) -> Result<()> {
    write_new(path, &serde_json::to_vec_pretty(value)?)
}

pub(crate) fn safe_relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "invalid relative evidence path"
    );
    Ok(path)
}

pub(crate) fn hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(root)?;
        if entry.file_type().is_file() && relative != Path::new("seal.json") {
            result.insert(
                relative.to_string_lossy().into_owned(),
                digest(&fs::read(entry.path())?),
            );
        } else {
            ensure!(
                !entry.file_type().is_symlink(),
                "symlink in sealed evidence: {}",
                relative.display()
            );
        }
    }
    Ok(result)
}

pub(crate) fn seal(root: &Path) -> Result<()> {
    // A hard-link publishes the complete manifest without replacing a prior seal.
    let pending = root.join(".seal-pending");
    let files = hashes(root)?;
    json_new(&pending, &json!({"schema_version":1,"files":files}))?;
    fs::hard_link(&pending, root.join("seal.json"))?;
    fs::remove_file(pending)?;
    #[cfg(unix)]
    fs::File::open(root)?.sync_all()?;
    freeze(root)?;
    Ok(())
}

pub(crate) fn freeze(root: &Path) -> Result<()> {
    for entry in WalkDir::new(root).contents_first(true).follow_links(false) {
        let entry = entry?;
        ensure!(
            !entry.file_type().is_symlink(),
            "symlink in read-only artifact"
        );
        let mut permissions = entry.metadata()?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() & !0o222);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(true);
        fs::set_permissions(entry.path(), permissions)?;
    }
    Ok(())
}

pub(crate) fn verify(root: &Path) -> Result<bool> {
    if !root.join("seal.json").exists() {
        return Ok(false);
    }
    let manifest = read_json(&root.join("seal.json"))?;
    let expected: BTreeMap<String, String> = serde_json::from_value(manifest["files"].clone())?;
    Ok(expected == hashes(root)?)
}

pub(crate) fn verify_manifest(root: &Path) -> Result<bool> {
    let bytes = fs::read(root.join("run.json"))?;
    let expected = fs::read_to_string(root.join("run.sha256"))?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    Ok(digest(&bytes) == expected.trim()
        && serde_json::to_value(hashes(&root.join("fixtures"))?)? == manifest["fixture_hashes"])
}

pub(crate) fn snapshot(workspace: &Path, task: &Value) -> Result<Value> {
    let mut files = BTreeMap::new();
    let mut errors = Vec::new();
    let mut names: Vec<_> = task["files"]
        .as_object()
        .context("task files")?
        .keys()
        .cloned()
        .collect();
    names.push(task["output"].as_str().context("task output")?.to_owned());
    if let Some(plan) = task["worker_plan_path"].as_str() {
        names.push(plan.to_owned());
    }
    names.extend(
        task["expected_outputs"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys().cloned()),
    );
    names.extend(
        task["absent_outputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned)),
    );
    names.sort();
    names.dedup();
    for name in names {
        let path = workspace.join(safe_relative(&name)?);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && meta.len() <= 2_000_000 => {
                // Reject parent symlinks too; never snapshot files outside the workspace.
                if !path.canonicalize()?.starts_with(workspace.canonicalize()?) {
                    errors.push(name);
                    continue;
                }
                match fs::read_to_string(&path) {
                    Ok(text) => {
                        files.insert(name, text);
                    }
                    Err(_) => errors.push(name),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            _ => errors.push(name),
        }
    }
    Ok(json!({"files":files,"errors":errors}))
}

pub(crate) fn worker_events(evidence: &Path) -> Result<Value> {
    Ok(serde_json::to_value(
        fs::read_to_string(evidence.join("worker-events.jsonl"))?
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )?)
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
