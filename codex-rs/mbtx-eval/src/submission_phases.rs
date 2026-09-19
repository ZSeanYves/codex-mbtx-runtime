//! Ordered, evaluator-owned input transitions for reusable-program validation.
//! Each phase has its own oracle and receipts; later phases retain program state.
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;

pub(crate) fn phases(case: &Value) -> Result<Vec<Value>> {
    match case.get("validation_phases") {
        None => Ok(vec![case.clone()]),
        Some(value) => {
            let phases = value
                .as_array()
                .context("validation phases must be an array")?;
            ensure!(!phases.is_empty(), "validation requires an initial phase");
            for phase in phases {
                ensure!(
                    phase["files"].is_object(),
                    "phase requires frozen input files"
                );
                ensure!(
                    phase["expected"].is_object(),
                    "phase requires an independent oracle"
                );
                ensure!(
                    phase["input_updates"].is_object(),
                    "phase requires explicit input updates"
                );
                ensure!(
                    phase.get("validation_phases").is_none(),
                    "nested validation phases are invalid"
                );
            }
            ensure!(
                phases[0]["files"] == case["files"],
                "initial phase input differs from the case"
            );
            ensure!(
                phases[0]["input_updates"]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty),
                "initial phase cannot update inputs"
            );
            Ok(phases.clone())
        }
    }
}

pub(crate) fn update_inputs(workspace: &Path, phase: &Value) -> Result<()> {
    for (name, content) in phase["input_updates"].as_object().into_iter().flatten() {
        let relative = crate::evidence::safe_relative(name)?;
        ensure!(
            !matches!(
                relative
                    .components()
                    .next()
                    .and_then(|c| c.as_os_str().to_str()),
                Some(
                    ".git"
                        | ".codex"
                        | ".codex-mbtx"
                        | "fixture-worker"
                        | "solution.sh"
                        | "solution.mbtx"
                )
            ),
            "phase cannot replace source or host metadata"
        );
        ensure!(
            phase["files"].get(name) == Some(content),
            "updated input must match frozen phase input"
        );
        let path = workspace.join(relative);
        let mut parent = workspace.to_path_buf();
        for part in relative.components() {
            parent.push(part);
            ensure!(
                !parent
                    .symlink_metadata()
                    .is_ok_and(|m| m.file_type().is_symlink()),
                "phase input cannot traverse a symlink"
            );
        }
        fs::create_dir_all(path.parent().context("phase input parent")?)?;
        fs::write(path, content.as_str().context("phase input must be text")?)?;
    }
    Ok(())
}

pub(crate) fn preserved_hashes(workspace: &Path, phase: &Value) -> Result<Value> {
    let mut hashes = serde_json::Map::new();
    for name in phase["preserve_previous"].as_array().into_iter().flatten() {
        let name = name.as_str().context("preserved phase path")?;
        let path = workspace.join(crate::evidence::safe_relative(name)?);
        let regular = path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file());
        let contained = path
            .canonicalize()
            .is_ok_and(|path| path.starts_with(workspace));
        let hash = if regular && contained {
            Value::String(crate::evidence::digest(&fs::read(path)?))
        } else {
            Value::Null
        };
        hashes.insert(name.to_owned(), hash);
    }
    Ok(Value::Object(hashes))
}

#[cfg(test)]
#[path = "submission_phases_tests.rs"]
mod tests;
