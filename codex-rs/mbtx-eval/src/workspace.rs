//! Per-attempt Git boundaries. Preparation is outside measured model work.
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use tokio::process::Command;

pub(crate) fn git_environment(command: &mut Command, workspace: &Path) {
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env(
            "GIT_CEILING_DIRECTORIES",
            workspace.parent().unwrap_or(workspace),
        );
}

/// Tools can read platform utilities and the frozen compiler/dependencies, but
/// cannot read run manifests, other attempts, or the parent checkout.
pub(crate) fn permissions(work: &Path, bundle: &Path, moon_home: &Path) -> Result<toml::Value> {
    let mut filesystem = toml::Table::new();
    filesystem.insert(":minimal".into(), "read".into());
    for path in [
        moon_home.join("bin"),
        moon_home.join("lib"),
        bundle.join("moon-home/registry"),
        bundle.join("dependencies"),
        Path::new("/opt/homebrew").into(),
        Path::new("/opt/local").into(),
    ] {
        if path.exists() {
            filesystem.insert(
                path.canonicalize()?.to_string_lossy().into_owned(),
                "read".into(),
            );
        }
    }
    for name in ["workspace", "home", "tmp"] {
        filesystem.insert(
            work.join(name)
                .canonicalize()?
                .to_string_lossy()
                .into_owned(),
            "write".into(),
        );
    }
    Ok(toml::Value::try_from(
        json!({"evaluation":{"filesystem":filesystem,"network":{"enabled":false}}}),
    )?)
}

pub(crate) async fn prepare(workspace: &Path, home: &Path, path: &str) -> Result<Value> {
    let started = std::time::Instant::now();
    let commands: &[&[&str]] = &[
        &[
            "init",
            "--quiet",
            "--initial-branch=experiment",
            "--template=",
        ],
        &["add", "--all"],
        &[
            "-c",
            "user.name=MBTX Evaluation",
            "-c",
            "user.email=evaluation@localhost",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "--allow-empty",
            "-m",
            "Fixture baseline",
        ],
        &["rev-parse", "--show-toplevel", "HEAD"],
    ];
    let mut baseline = String::new();
    for args in commands {
        let mut command = Command::new("git");
        command
            .args(*args)
            .current_dir(workspace)
            .env_clear()
            .env("PATH", path)
            .env("HOME", home)
            .env("LANG", "C")
            .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z");
        git_environment(&mut command, workspace);
        let output = command
            .output()
            .await
            .context("initialize isolated fixture repository")?;
        ensure!(
            output.status.success(),
            "fixture Git preparation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args[0] == "init" {
            fs::create_dir_all(workspace.join(".git/info"))?;
            fs::write(
                workspace.join(".git/info/exclude"),
                "/.codex/\n/.agents/\n/.codex-mbtx/\n/fixture-worker\n",
            )?;
        }
        if args[0] == "rev-parse" {
            baseline = String::from_utf8(output.stdout)?;
        }
    }
    let mut lines = baseline.lines();
    let root = lines.next().context("Git root")?;
    ensure!(
        Path::new(root).canonicalize()? == workspace.canonicalize()?,
        "Git discovered an outer repository"
    );
    Ok(
        json!({"git_root":root,"baseline_commit":lines.next().context("baseline commit")?,"preparation_ns":started.elapsed().as_nanos() as u64,"global_and_system_config":false}),
    )
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
