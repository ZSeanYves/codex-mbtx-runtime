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

/// The immutable model catalog with provider metadata preserved as published.
pub(crate) const NATIVE_CODE_MODE_CATALOG: &str = "models-code-mode.json";
/// A harness-controlled catalog that differs only in the requested tool mode.
pub(crate) const HARNESS_DIRECT_CATALOG: &str = "models-direct.json";

/// Build the two catalog variants used by the four-condition experiment.
///
/// The native variant preserves all source metadata. The direct variant only
/// changes `tool_mode` for the target model; every other model field remains intact.
pub(crate) fn catalog_variants(catalog: &Value) -> Result<(Value, Value)> {
    let models = catalog["models"].as_array().context("model catalog")?;
    ensure!(!models.is_empty(), "model catalog is empty");
    ensure!(
        models.iter().any(|model| {
            model["slug"].as_str() == Some("gpt-5.6-terra")
                && model["tool_mode"].as_str() == Some("code_mode_only")
        }),
        "native model catalog lacks gpt-5.6-terra code_mode_only metadata"
    );

    let mut direct = catalog.clone();
    let mut target_count = 0;
    for model in direct["models"].as_array_mut().context("model catalog")? {
        if model["slug"].as_str() == Some("gpt-5.6-terra") {
            target_count += 1;
            model
                .as_object_mut()
                .context("model catalog entry")?
                .insert("tool_mode".to_owned(), Value::String("direct".to_owned()));
        }
    }
    ensure!(
        target_count == 1,
        "native model catalog has duplicate gpt-5.6-terra entries"
    );
    Ok((catalog.clone(), direct))
}

fn catalog_metadata(path: &Path, variant: &str, provenance: &str) -> Result<Value> {
    let bytes = fs::read(path)?;
    let file = path
        .file_name()
        .context("catalog filename")?
        .to_string_lossy();
    Ok(json!({
        "file": file,
        "variant": variant,
        "provenance": provenance,
        "sha256": digest(&bytes),
    }))
}

fn condition_manifest(catalogs: &Value, policy_sha256: &str) -> Value {
    let catalog_path = |variant: &str| catalogs[variant]["file"].clone();
    let catalog_sha256 = |variant: &str| catalogs[variant]["sha256"].clone();
    let mut conditions = Vec::new();
    for profile in ["production", "capability_matched"] {
        for execution_mode in ["direct", "code_mode"] {
            for backend_arm in ["shell", "mbtx"] {
                let code_mode = execution_mode == "code_mode";
                let mbtx = backend_arm == "mbtx";
                let catalog_variant = if code_mode {
                    "native_code_mode"
                } else {
                    "harness_direct"
                };
                let condition_id = format!(
                    "{}-{backend_arm}-{}",
                    execution_mode.replace('_', "-"),
                    profile.replace('_', "-")
                );
                let model_visible_tools = if code_mode {
                    json!(["exec", "wait"])
                } else if mbtx {
                    json!(["mbtx"])
                } else {
                    json!(["exec_command", "write_stdin"])
                };
                let expected_nested_tools = if code_mode {
                    if mbtx {
                        json!(["mbtx"])
                    } else {
                        json!(["exec_command", "write_stdin"])
                    }
                } else {
                    json!([])
                };
                conditions.push(json!({
                    "condition_id": condition_id,
                    "execution_mode": execution_mode,
                    "backend_arm": backend_arm,
                    "catalog_variant": catalog_variant,
                    "catalog_file": catalog_path(catalog_variant),
                    "catalog_sha256": catalog_sha256(catalog_variant),
                    "policy_sha256": policy_sha256,
                    "capability_profile": profile,
                    "requested_tool_mode": if code_mode { "code_mode_only" } else { "direct" },
                    "effective_tool_mode": if code_mode { "code_mode_only" } else { "direct" },
                    "shell_type": "unified_exec",
                    "expected_direct_tools": model_visible_tools.clone(),
                    "model_visible_tools": model_visible_tools,
                    "expected_nested_tools": expected_nested_tools,
                    "capability_matching": if profile == "capability_matched" {
                        if mbtx { "policy_admitted" } else { "shell_admission_unverified" }
                    } else {
                        "production_boundary"
                    },
                    "features": {
                        "shell_tool": !mbtx,
                        "unified_exec": !mbtx,
                        "code_mode": true,
                        "code_mode_only": false,
                        "code_mode_host": true,
                        "code_mode_prewarm": true,
                        "code_mode_interrupt": true,
                    },
                }));
            }
        }
    }
    json!({
        "schema_version": 1,
        "catalogs": catalogs,
        "conditions": conditions,
    })
}

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
    fs::copy(
        repo.join("codex-rs/target/debug/codex-code-mode-host"),
        pending.join("codex-code-mode-host"),
    )?;
    // MoonRun's host PATH intentionally excludes general utilities. The Linux
    // sandbox must resolve its existing launcher independently of that PATH.
    #[cfg(target_os = "linux")]
    {
        fs::create_dir(pending.join("codex-resources"))?;
        fs::copy(
            which::which("bwrap").context("Linux evaluation requires bubblewrap")?,
            pending.join("codex-resources/bwrap"),
        )?;
    }
    fs::copy(std::env::current_exe()?, pending.join("mbtx-eval"))?;
    fs::copy(
        repo.join("codex-rs/target/debug/mbtx-fixture-worker"),
        pending.join("fixture-worker"),
    )?;
    fs::copy(&moonrun, pending.join("moonrun"))?;
    fs::copy(
        repo.join("mbtx/fixtures/process-policy.mbtx"),
        pending.join("process-policy.mbtx"),
    )?;
    // Version resolution reads registry metadata even for a frozen build. Keep
    // that metadata immutable without exposing the user's MoonBit credentials.
    let registry = moon_home.join("registry/index");
    for name in ["async", "core"] {
        let relative = format!("user/moonbitlang/{name}.index");
        let target = pending.join("moon-home/registry/index").join(&relative);
        fs::create_dir_all(target.parent().context("registry parent")?)?;
        fs::copy(registry.join(relative), target)?;
    }
    fs::create_dir(pending.join("reference"))?;
    for name in [
        "moonbit.md",
        "shell.md",
        "tools.md",
        "examples.mbtx",
        "syntax.md",
        "files.md",
        "collections.md",
        "processes.md",
        "process-examples.mbtx",
    ] {
        fs::copy(
            repo.join("mbtx/reference").join(name),
            pending.join("reference").join(name),
        )?;
    }
    fs::copy(
        repo.join("codex-rs/ext/mbtx/src/example.mbtx"),
        pending.join("reference/tool-example.mbtx"),
    )?;
    fs::copy(
        repo.join("mbtx/_build/wasm/release/build/cmd/evaluation-model/evaluation-model.wasm"),
        pending.join("evaluation-model.wasm"),
    )?;
    let source_catalog_path = repo.join("codex-rs/models-manager/models.json");
    let source_catalog = read_json(&source_catalog_path)?;
    let (native_catalog, direct_catalog) = catalog_variants(&source_catalog)?;
    // Keep models.json as a compatibility alias for callers that predate the
    // condition manifest. New runs select one of the explicit variant files.
    json_new(&pending.join("models.json"), &native_catalog)?;
    json_new(&pending.join(NATIVE_CODE_MODE_CATALOG), &native_catalog)?;
    json_new(&pending.join(HARNESS_DIRECT_CATALOG), &direct_catalog)?;
    let catalogs = json!({
        "native_code_mode": catalog_metadata(
            &pending.join(NATIVE_CODE_MODE_CATALOG),
            "native_code_mode",
            "native provider metadata",
        )?,
        "harness_direct": catalog_metadata(
            &pending.join(HARNESS_DIRECT_CATALOG),
            "harness_direct",
            "harness-controlled variant",
        )?,
    });
    let policy_sha256 = digest(&fs::read(pending.join("process-policy.mbtx"))?);
    let conditions = condition_manifest(&catalogs, &policy_sha256);
    json_new(&pending.join("conditions.json"), &conditions)?;
    // Publish dependency sources, not runtime locks or generated checks. The host
    // materializes an invocation-local cache and Moon creates its own lock.
    // async 0.21.3 has no external dependencies. Core ships with the compiler.
    // Expanding this allowlist requires a reviewed dependency-closure change.
    let relative_dependency = "moonbitlang/async/0.21.3";
    let deps = moon_home
        .join("cache/deps/v1/sources")
        .join(relative_dependency);
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
        let destination = pending
            .join("dependencies/v1/sources")
            .join(relative_dependency)
            .join(relative);
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
    let info = json!({"schema_version":1,"fingerprint":fingerprint,"source_revision":String::from_utf8_lossy(&revision.stdout).trim(),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"profile":"dev-debug0","target":"wasm","moon_path":moon,"moonrun_path":moonrun,"moon_home":moon_home,"moon_version":version(&moon,"version")?,"moonrun_version":version(&moonrun,"--version")?,"moon_sha256":digest(&fs::read(&moon)?),"moonrun_sha256":digest(&fs::read(&moonrun)?),"catalog_source":source_catalog_path,"catalogs":catalogs,"conditions":conditions,"files":hashes(&pending)?});
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

pub(crate) fn verify_analysis(bundle: &Path) -> Result<Value> {
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
    verify_catalogs(bundle, &info)?;
    Ok(info)
}

fn verify_catalogs(bundle: &Path, info: &Value) -> Result<()> {
    if info.get("catalogs").is_none() {
        // Bundles assembled before the four-condition catalog split remain
        // analyzable for historical reports. New bundles always carry this
        // metadata and take the checks below.
        return Ok(());
    }
    for variant in ["native_code_mode", "harness_direct"] {
        let catalog = &info["catalogs"][variant];
        let file = catalog["file"].as_str().context("catalog filename")?;
        let bytes = fs::read(bundle.join(file))?;
        ensure!(
            digest(&bytes) == catalog["sha256"].as_str().context("catalog hash")?,
            "catalog hash mismatch for {variant}"
        );
    }
    let source = read_json(&bundle.join(NATIVE_CODE_MODE_CATALOG))?;
    let direct = read_json(&bundle.join(HARNESS_DIRECT_CATALOG))?;
    let (_, expected_direct) = catalog_variants(&source)?;
    ensure!(
        direct == expected_direct,
        "direct catalog differs from native catalog outside tool_mode"
    );
    ensure!(
        info["conditions"]["catalogs"] == info["catalogs"],
        "condition manifest catalog metadata mismatch"
    );
    Ok(())
}

pub(crate) fn verify(bundle: &Path) -> Result<Value> {
    let info = verify_analysis(bundle)?;
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

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
