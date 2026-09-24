use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use std::path::Path;

pub(crate) fn process_instructions(task: &Value, arm: &str) -> Result<String> {
    if arm != "mbtx_program" || task["cohort"] != "natural-tool-choice" {
        return Ok(String::new());
    }
    let rules = crate::process_policy::rules(task)?.context("natural process rules")?;
    Ok(format!(
        "\n\nMBTX direct-process contract (generated from the enforced policy):\n{}\nUse a bare program name and literal argv. Each args_prefix is mandatory and must appear at the beginning of argv in exactly this order; append any positional operands afterwards. An empty prefix permits that program with any literal arguments within task rules. All other programs/forms and absolute executable paths are denied. For rg, the --no-config and -- separators are required, including file listing: @shell.Cmd(\"rg\", [\"--no-config\", \"--files\", \"--\"]). Search uses [\"--no-config\", \"--json\", \"--\", PATTERN, PATH]. A denied form does not mean all process spawning is unavailable; correct the argv or use the filesystem API. Do not invoke interpreters indirectly through utility features.",
        serde_json::to_string_pretty(rules)?
    ))
}

pub(crate) fn record(directory: &Path, task: &Value, instructions: &str) -> Result<()> {
    if task["cohort"] != "natural-tool-choice" {
        return Ok(());
    }
    let goal = task["goal"].as_str().context("public task goal")?;
    ensure!(
        goal.len() <= 16_384,
        "public task goal exceeds 16 KiB limit"
    );
    ensure!(
        instructions.len() <= 16_384,
        "task instructions exceed 16 KiB limit"
    );
    crate::evidence::json_new(
        &directory.join("task-contract.json"),
        &json!({
            "public_goal": goal,
            "public_instructions": instructions,
            "instructions_sha256": crate::evidence::digest(instructions.as_bytes()),
            "process_allow_sha256": crate::evidence::digest(&serde_json::to_vec(&task["process_allow"])?),
            "prompt_sha256": crate::evidence::digest(goal.as_bytes()),
            "output_contract": task["output_contract"],
            "output_contract_sha256": crate::evidence::digest(&serde_json::to_vec(&task["output_contract"])?),
            "oracle_sha256": crate::evidence::digest(&serde_json::to_vec(&json!({"expected":task["expected"],"expected_outputs":task["expected_outputs"]}))?),
        }),
    )
}

pub(crate) fn validate(body: &Value, receipt: &Value) -> Result<()> {
    let goal = receipt["public_goal"]
        .as_str()
        .context("recorded public goal")?;
    ensure!(
        receipt["prompt_sha256"] == crate::evidence::digest(goal.as_bytes()),
        "public task prompt hash mismatch"
    );
    ensure!(
        body["input"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| {
                item["role"] == "user"
                    && item["content"]
                        .as_array()
                        .is_some_and(|parts| parts.iter().any(|part| part["text"] == goal))
            })),
        "outbound request omits or changes the frozen public task contract"
    );
    if let Some(instructions) = receipt["public_instructions"].as_str() {
        ensure!(
            receipt["instructions_sha256"] == crate::evidence::digest(instructions.as_bytes()),
            "task instructions hash mismatch"
        );
        ensure!(
            body["input"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| {
                    item["role"] == "developer"
                        && item["content"].as_array().is_some_and(|parts| {
                            parts.iter().any(|part| {
                                part["text"]
                                    .as_str()
                                    .is_some_and(|text| text.contains(instructions))
                            })
                        })
                })),
            "outbound request omits or changes the task process instructions"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "task_contract_tests.rs"]
mod tests;
