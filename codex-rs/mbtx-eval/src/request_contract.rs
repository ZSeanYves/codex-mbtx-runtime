use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;

/// Check the assigned mode and backend before forwarding each Responses request.
/// The nested inventory comes from the host-generated declarations in exec's
/// actual tool description, never from a backend name guessed from model calls.
pub(crate) fn validate(body: &Value, arm: &str, execution_mode: &str) -> Result<()> {
    if arm == "probe" {
        return Ok(());
    }
    ensure!(
        matches!(execution_mode, "direct" | "code_mode"),
        "unknown execution mode: {execution_mode}"
    );
    ensure!(
        matches!(arm, "shell_tool" | "mbtx_program"),
        "unknown evaluation arm: {arm}"
    );
    let code_mode = execution_mode == "code_mode";
    let mut pending = Vec::new();
    if let Some(tools) = body.get("tools") {
        pending.extend(tools.as_array().context("tools must be an array")?);
    }
    let input = body["input"]
        .as_array()
        .context("task input must be an array")?;
    for item in input {
        if item["type"] == "additional_tools" {
            pending.extend(
                item["tools"]
                    .as_array()
                    .context("additional_tools must contain tools")?,
            );
        }
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
    let mut outer = BTreeSet::new();
    let mut nested = BTreeSet::new();
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
        outer.insert(name);
        if name == "exec" {
            let description = tool["description"]
                .as_str()
                .context("exec nested tool description missing")?;
            for declaration in description.split("declare const tools: { ").skip(1) {
                let name = declaration
                    .split_once('(')
                    .context("malformed nested tool declaration")?
                    .0;
                ensure!(
                    !name.is_empty()
                        && name
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
                    "malformed nested tool name"
                );
                nested.insert(name);
            }
        }
    }
    for name in outer.iter().chain(nested.iter()) {
        ensure!(
            matches!(
                *name,
                "mbtx"
                    | "exec_command"
                    | "write_stdin"
                    | "exec"
                    | "wait"
                    | "read_resource"
                    | "apply_patch"
                    | "view_image"
                    | "request_user_input"
                    | "get_goal"
                    | "create_goal"
                    | "update_goal"
            ),
            "tool {name} is not allowed in {arm}"
        );
    }
    let backend = if code_mode {
        ensure!(
            outer.contains("exec") && outer.contains("wait"),
            "Code Mode requires both exec and wait; refusing Direct fallback"
        );
        ensure!(
            outer
                .iter()
                .all(|name| matches!(*name, "exec" | "wait" | "request_user_input")),
            "Code Mode exposes a direct backend tool"
        );
        ensure!(
            !nested.contains("exec") && !nested.contains("wait"),
            "Code Mode orchestration recursively exposed as nested tools"
        );
        &nested
    } else {
        ensure!(
            !outer.contains("exec") && !outer.contains("wait"),
            "Direct condition exposes Code Mode exec/wait"
        );
        &outer
    };
    match arm {
        "mbtx_program" => ensure!(
            backend.contains("mbtx")
                && !backend.contains("exec_command")
                && !backend.contains("write_stdin"),
            "MBTX backend absent or Shell tool leaked into MBTX condition"
        ),
        "shell_tool" => ensure!(
            backend.contains("exec_command")
                && backend.contains("write_stdin")
                && !backend.contains("mbtx"),
            "unified Shell backend requires exec_command/write_stdin and excludes mbtx"
        ),
        _ => unreachable!("arm was checked above"),
    }
    Ok(())
}

/// Prescribed offline replies must use the advertised execution path. Real model
/// mistakes stay in history as tool errors; they must not become harness faults.
pub(crate) fn validate_replay_history(body: &Value, arm: &str, execution_mode: &str) -> Result<()> {
    let code_mode = execution_mode == "code_mode";
    for item in body["input"].as_array().context("replay input")? {
        if matches!(
            item["type"].as_str(),
            Some("function_call" | "custom_tool_call")
        ) {
            let name = item["name"]
                .as_str()
                .context("historical tool name missing")?;
            ensure!(
                !code_mode || matches!(name, "exec" | "wait" | "request_user_input"),
                "direct tool call {name} leaked into Code Mode replay"
            );
            ensure!(
                code_mode || !matches!(name, "exec" | "wait"),
                "Code Mode call {name} leaked into Direct replay"
            );
            ensure!(
                arm != "mbtx_program"
                    || !matches!(
                        name,
                        "exec_command" | "write_stdin" | "shell" | "shell_command"
                    ),
                "Shell call leaked into MBTX replay"
            );
            ensure!(
                arm != "shell_tool" || name != "mbtx",
                "MBTX call leaked into Shell replay"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "request_contract_tests.rs"]
mod tests;
