//! Frozen, input-driven references used only by prescribed offline replay.
use anyhow::Context;
use anyhow::Result;
use serde_json::Value;

pub(crate) fn source(task: &Value) -> Result<String> {
    let source = task["reference_mbtx"]
        .as_str()
        .context("task has no frozen MBTX reference for offline replay")?;
    anyhow::ensure!(
        source.len() <= crate::submission_contract::SOURCE_LIMIT_BYTES as usize,
        "reference source exceeds delivery limit"
    );
    Ok(source.to_owned())
}

#[cfg(test)]
#[path = "replay_program_tests.rs"]
mod tests;
