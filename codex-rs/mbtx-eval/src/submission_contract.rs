//! One arm-specific source contract for prompts, capture and prescribed replay.
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;

pub(crate) const SOURCE_LIMIT_BYTES: u64 = 65_536;

pub(crate) fn required_source(task: &Value, arm: &str) -> Result<Option<String>> {
    let expected = match arm {
        "shell_tool" => "solution.sh",
        "mbtx_program" => "solution.mbtx",
        _ => anyhow::bail!("unknown submission arm {arm}"),
    };
    match task.get("required_source") {
        None => Ok((task["acceptance"] == "programs").then(|| expected.to_owned())),
        Some(Value::Null) => {
            ensure!(
                task["acceptance"] != "programs",
                "delivery task has no source contract"
            );
            Ok(None)
        }
        Some(value) => {
            ensure!(
                task["acceptance"] == "programs",
                "workflow task declares delivery source"
            );
            ensure!(
                value[arm].as_str() == Some(expected),
                "invalid exact source contract for {arm}"
            );
            Ok(Some(expected.to_owned()))
        }
    }
}

#[cfg(test)]
#[path = "submission_contract_tests.rs"]
mod tests;
