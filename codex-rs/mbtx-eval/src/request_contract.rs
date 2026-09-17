use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;

/// Validate both Responses tool encodings before forwarding any task request.
pub(crate) fn validate(body: &Value, arm: &str) -> Result<()> {
    if arm == "probe" {
        return Ok(());
    }
    let mut pending = Vec::new();
    if let Some(tools) = body.get("tools") {
        pending.extend(tools.as_array().context("tools must be an array")?);
    }
    for item in body["input"]
        .as_array()
        .context("task input must be an array")?
    {
        if item["type"] == "additional_tools" {
            pending.extend(
                item["tools"]
                    .as_array()
                    .context("additional_tools must contain tools")?,
            );
        }
        // Isolated config should suppress these inherited context fragments.
        if matches!(item["role"].as_str(), Some("developer" | "user")) {
            for part in item["content"].as_array().into_iter().flatten() {
                let text = part["text"].as_str().unwrap_or_default();
                ensure!(
                    !text.starts_with("# AGENTS.md instructions")
                        && !text.contains("<skills_instructions>"),
                    "inherited instructions leaked into evaluation"
                );
            }
        }
    }
    let mut execution_present = false;
    while let Some(tool) = pending.pop() {
        if tool["type"] == "namespace" {
            ensure!(
                tool["name"] == "functions",
                "unexpected tool namespace: {}",
                tool["name"]
            );
            pending.extend(tool["tools"].as_array().context("namespace tools")?);
            continue;
        }
        let name = tool["name"].as_str().context("tool name missing")?;
        let execution = match arm {
            "mbtx_program" => name == "mbtx",
            "shell_tool" => matches!(name, "exec_command" | "shell" | "shell_command"),
            _ => anyhow::bail!("unknown evaluation arm: {arm}"),
        };
        execution_present |= execution;
        ensure!(
            execution
                || (arm == "shell_tool" && name == "write_stdin")
                || matches!(
                    name,
                    "read_resource" | "apply_patch"
                        | "view_image"
                        | "request_user_input"
                        | "get_goal"
                        | "create_goal"
                        | "update_goal"
                ),
            "tool {name} is not allowed in {arm}"
        );
    }
    ensure!(
        execution_present,
        "assigned execution tool is absent in {arm}"
    );
    Ok(())
}

#[cfg(test)]
#[path = "request_contract_tests.rs"]
mod tests;
