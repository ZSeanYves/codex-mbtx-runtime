//! Flat attempt export retains unknown observations and unstarted assignments.
use super::cell;
use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

pub(super) fn rows(model: &Value) -> Result<Vec<Vec<String>>> {
    let mut rows = vec![vec![
        "Task".into(),
        "Scenario".into(),
        "Input variant".into(),
        "Arm".into(),
        "Status".into(),
        "Oracle".into(),
        "Started steps".into(),
        "Accepted steps".into(),
        "Native request starts".into(),
        "Observed upstream sends".into(),
        "Tool calls".into(),
        "Invocation failures".into(),
        "MBTX compile failures".into(),
        "MBTX execution failures".into(),
        "Shell command failures".into(),
        "Shared edit calls".into(),
        "Evidence".into(),
        "Cohort".into(),
        "Complexity".into(),
        "Pair".into(),
        "Repeat".into(),
        "Steps to success".into(),
        "Observed compiler starts".into(),
        "Observed program launches".into(),
        "Observed process denial diagnostics".into(),
        "Child process count".into(),
        "Session terminal".into(),
        "Delivery status".into(),
        "MBTX result coverage".into(),
        "Observed denial diagnostic count".into(),
        "Delivery compiler starts".into(),
        "Delivery program starts".into(),
        "Track".into(),
        "First successful compile decision".into(),
        "Observed reference first pages".into(),
        "Observed reference continuation pages".into(),
        "Observed output continuation pages".into(),
        "Resource reads with unknown identity".into(),
        "HTTP transport retries".into(),
        "Agent stream retries".into(),
        "Observed decision prefix".into(),
        "Observed started decision prefix".into(),
        "Track assigned denominator".into(),
        "Track successes".into(),
    ]];
    for a in model["attempts"].as_array().context("attempts")? {
        rows.push(vec![
            cell(&a["task_id"]),
            cell(&a["scenario"]),
            cell(&a["variant"]),
            cell(&a["arm"]),
            cell(&a["status"]),
            cell(&a["oracle"]["success"]),
            cell(&a["accounting"]["metrics"]["agent_steps_started"]),
            cell(&a["accounting"]["metrics"]["agent_steps"]),
            cell(&a["accounting"]["metrics"]["model_requests"]),
            cell(&a["upstream_requests_observed"]),
            cell(&a["accounting"]["metrics"]["tool_calls"]),
            cell(&a["accounting"]["metrics"]["tool_errors"]),
            cell(&a["tool_outcomes"]["mbtx_compile_failures"]),
            cell(&a["tool_outcomes"]["mbtx_execution_failures"]),
            cell(&a["tool_outcomes"]["shell_command_failures"]),
            cell(&a["tool_outcomes"]["shared_edit_calls"]),
            cell(&a["evidence"]["directory"]),
            cell(&a["cohort"]),
            cell(&a["complexity"]),
            cell(&a["pair_id"]),
            cell(
                &model["pairs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|p| p["pair_id"] == a["pair_id"])
                    .unwrap_or(&Value::Null)["repeat"],
            ),
            cell(&a["steps_to_success"]),
            cell(&a["tool_outcomes"]["mbtx_compilations_observed"]),
            cell(&a["tool_outcomes"]["mbtx_program_launches_observed"]),
            cell(&a["tool_outcomes"]["process_denial_diagnostics_observed"]),
            cell(&a["tool_outcomes"]["child_process_count"]),
            cell(&a["session_terminal"]),
            cell(&a["submission"]["status"]),
            cell(&a["tool_outcomes"]["mbtx_result_coverage"]),
            cell(&json!(
                a["tool_outcomes"]["process_denial_diagnostics_observed"]
                    .as_array()
                    .map(Vec::len)
            )),
            cell(&a["submission"]["phase_counts"]["compiler_starts"]),
            cell(&a["submission"]["phase_counts"]["program_starts"]),
            cell(&a["track"]),
            cell(&a["interaction"]["first_successful_compile_step"]),
            cell(&a["interaction"]["reference_first_pages_observed"]),
            cell(&a["interaction"]["reference_continuations_observed"]),
            cell(&a["interaction"]["output_continuations_observed"]),
            cell(&a["interaction"]["resource_reads_unknown"]),
            cell(&a["accounting"]["metrics"]["http_transport_retries"]),
            cell(&a["accounting"]["metrics"]["agent_stream_retries"]),
            cell(&a["accounting"]["observed"]["agent_steps"]),
            cell(&a["accounting"]["observed"]["agent_steps_started"]),
            cell(&track_arm(model, &a["track"], &a["arm"])["assigned"]),
            cell(&track_arm(model, &a["track"], &a["arm"])["successes"]),
        ]);
    }
    for pair in model["pairs"].as_array().into_iter().flatten() {
        for arm in ["shell_tool", "mbtx_program"] {
            if model["attempts"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|a| a["pair_id"] == pair["pair_id"] && a["arm"] == arm)
            {
                continue;
            }
            let mut row = vec!["null".to_owned(); rows[0].len()];
            for (index, value) in [
                (0, cell(&pair["task_id"])),
                (1, cell(&pair["scenario"])),
                (2, cell(&pair["variant"])),
                (3, arm.into()),
                (4, "not_started".into()),
                (17, cell(&pair["cohort"])),
                (18, cell(&pair["complexity"])),
                (19, cell(&pair["pair_id"])),
                (20, cell(&pair["repeat"])),
                (32, cell(&pair["track"])),
                (
                    42,
                    cell(&track_arm(model, &pair["track"], &json!(arm))["assigned"]),
                ),
                (
                    43,
                    cell(&track_arm(model, &pair["track"], &json!(arm))["successes"]),
                ),
            ] {
                row[index] = value;
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

fn track_arm<'a>(model: &'a Value, track: &Value, arm: &Value) -> &'a Value {
    model["by_track"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|group| group["name"] == *track)
        .map_or(&Value::Null, |group| {
            &group["itt"][arm.as_str().unwrap_or("")]
        })
}
