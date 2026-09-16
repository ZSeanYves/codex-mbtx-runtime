use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use walkdir::WalkDir;

use crate::evidence::digest;
use crate::evidence::hashes;
use crate::evidence::json_new;
use crate::evidence::read_json;

fn version(program: &Path, arg: &str) -> Result<String> {
    let output = Command::new(program).arg(arg).output()?;
    ensure!(
        output.status.success(),
        "tool version check failed: {}",
        program.display()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn toolchain_hashes(root: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    let mut result = std::collections::BTreeMap::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some(".git" | ".DS_Store")))
    {
        let entry = entry?;
        if entry.file_type().is_file() {
            result.insert(
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .into_owned(),
                digest(&fs::read(entry.path())?),
            );
        }
    }
    Ok(result)
}

pub(crate) fn assemble(repo: &Path, output: &Path, fingerprint: &str) -> Result<()> {
    ensure!(
        !output.exists(),
        "bundle already exists; verify or choose a fresh directory"
    );
    fs::create_dir_all(output.parent().context("bundle parent")?)?;
    let pending = output.with_extension(format!("pending-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&pending)?;
    let moon = which::which("moon")?.canonicalize()?;
    let moonrun = which::which("moonrun")?.canonicalize()?;
    let moon_home = moon
        .parent()
        .and_then(Path::parent)
        .context("MoonBit installation root")?;
    fs::copy(
        repo.join("codex-rs/target/debug/codex"),
        pending.join("codex"),
    )?;
    fs::copy(std::env::current_exe()?, pending.join("mbtx-eval"))?;
    fs::copy(&moonrun, pending.join("moonrun"))?;
    fs::copy(
        repo.join("mbtx/_build/wasm/release/build/cmd/evaluation-model/evaluation-model.wasm"),
        pending.join("evaluation-model.wasm"),
    )?;
    let mut catalog = read_json(&repo.join("codex-rs/models-manager/models.json"))?;
    // Both arms use the same frozen native-tools catalog. Preserve all other
    // model capabilities; verify the actual transmitted tool schemas in the gate.
    for model in catalog["models"].as_array_mut().context("model catalog")? {
        model["tool_mode"] = json!("direct");
    }
    json_new(&pending.join("models.json"), &catalog)?;
    // Publish dependency sources, not runtime locks or generated checks. The host
    // materializes an invocation-local cache and Moon creates its own lock.
    let deps = moon_home.join("cache/deps/v1/sources");
    fs::create_dir_all(pending.join("dependencies"))?;
    // This format marker is required even by --frozen. The runtime lock is not.
    fs::copy(
        moon_home.join("cache/deps/.moon-cache"),
        pending.join("dependencies/.moon-cache"),
    )?;
    for entry in WalkDir::new(&deps)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some("_build" | ".git" | ".DS_Store")
            )
        })
    {
        let entry = entry?;
        let relative = entry.path().strip_prefix(&deps)?;
        let destination = pending.join("dependencies/v1/sources").join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), destination)?;
        } else {
            anyhow::bail!("unsupported dependency-cache symlink");
        }
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()?;
    let info = json!({"schema_version":1,"fingerprint":fingerprint,"source_revision":String::from_utf8_lossy(&revision.stdout).trim(),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"profile":"dev-debug0","target":"wasm","moon_path":moon,"moonrun_path":moonrun,"moon_home":moon_home,"moon_version":version(&moon,"version")?,"moonrun_version":version(&moonrun,"--version")?,"moon_sha256":digest(&fs::read(&moon)?),"moonrun_sha256":digest(&fs::read(&moonrun)?),"files":hashes(&pending)?});
    let mut info = info;
    let worktree = Command::new("git")
        .args(["status", "--porcelain", "--", "codex-rs", "mbtx"])
        .current_dir(repo)
        .output()?;
    ensure!(
        revision.status.success() && worktree.status.success(),
        "source provenance command failed"
    );
    info["source_worktree_status"] = json!(String::from_utf8_lossy(&worktree.stdout));
    info["moon_bin_hashes"] = serde_json::to_value(toolchain_hashes(&moon_home.join("bin"))?)?;
    info["moon_lib_hashes"] = serde_json::to_value(toolchain_hashes(&moon_home.join("lib"))?)?;
    json_new(&pending.join("bundle.json"), &info)?;
    crate::evidence::freeze(&pending)?;
    fs::rename(pending, output)?;
    Ok(())
}

pub(crate) fn verify(bundle: &Path) -> Result<Value> {
    let info = read_json(&bundle.join("bundle.json"))?;
    ensure!(info["schema_version"] == 1, "unsupported bundle schema");
    ensure!(
        info["platform"] == std::env::consts::OS && info["architecture"] == std::env::consts::ARCH,
        "bundle platform differs from this host"
    );
    ensure!(
        digest(&fs::read(std::env::current_exe()?)?) == info["files"]["mbtx-eval"],
        "use the mbtx-eval executable inside the selected bundle"
    );
    let mut actual = hashes(bundle)?;
    actual.remove("bundle.json");
    ensure!(
        serde_json::to_value(actual)? == info["files"],
        "bundle hash mismatch"
    );
    for (path, key) in [
        ("moon_path", "moon_sha256"),
        ("moonrun_path", "moonrun_sha256"),
    ] {
        ensure!(
            digest(&fs::read(info[path].as_str().context("tool path")?)?) == info[key],
            "installed MoonBit tool changed; prepare a new bundle"
        );
    }
    let home = Path::new(info["moon_home"].as_str().context("MoonBit root")?);
    ensure!(
        serde_json::to_value(toolchain_hashes(&home.join("bin"))?)? == info["moon_bin_hashes"]
            && serde_json::to_value(toolchain_hashes(&home.join("lib"))?)?
                == info["moon_lib_hashes"],
        "MoonBit compiler or core libraries changed; prepare a new bundle"
    );
    Ok(info)
}
