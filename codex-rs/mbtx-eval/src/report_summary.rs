//! Descriptive calibration tables; repetitions never increase the task count.
use serde_json::Value;

pub(super) fn markdown(model: &Value) -> String {
    let mut output = String::new();
    let tracks = model["by_track"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| {
            ["shell_tool", "mbtx_program"].map(|arm| {
                [
                    super::cell(&group["name"]),
                    arm.into(),
                    super::cell(&group["itt"][arm]["captured"]),
                    super::cell(&group["itt"][arm]["assigned"]),
                    super::cell(&group["itt"][arm]["successes"]),
                    super::cell(&group["conditional"]["pairs"]),
                    super::cell(&group["conditional"]["mean_step_difference"]),
                ]
                .to_vec()
            })
        })
        .collect();
    append_table(
        &mut output,
        "Basic and complexity tracks",
        &[
            "Track",
            "Arm",
            "Captured",
            "Assigned denominator",
            "Successful",
            "Comparable successful pairs",
            "Conditional MBTX minus Shell steps",
        ],
        tracks,
    );
    let interactions = model["attempts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["interaction"].is_object())
        .map(|a| {
            let interaction = &a["interaction"];
            [
                super::cell(&a["track"]),
                super::cell(&a["task_id"]),
                super::cell(&a["arm"]),
                super::cell(&a["pair_id"]),
                super::cell(&interaction["first_successful_compile_step"]),
                super::cell(&interaction["reference_first_pages_observed"]),
                super::cell(&interaction["reference_continuations_observed"]),
                super::cell(&interaction["output_continuations_observed"]),
                super::cell(&a["tool_outcomes"]["mbtx_compile_failures"]),
                super::cell(&a["tool_outcomes"]["mbtx_execution_failures"]),
                super::cell(&a["accounting"]["metrics"]["http_transport_retries"]),
                super::cell(&a["accounting"]["metrics"]["agent_stream_retries"]),
            ]
            .to_vec()
        })
        .collect();
    append_table(
        &mut output,
        "Observed interaction costs",
        &[
            "Track",
            "Task",
            "Arm",
            "Pair",
            "First successful compile decision",
            "Reference first pages",
            "Reference continuation pages",
            "Output continuation pages",
            "Compile failures",
            "Runtime failures",
            "HTTP retries",
            "Stream retries",
        ],
        interactions,
    );
    for (key, title, fields) in [
        (
            "track_step_distributions",
            "Actual steps by track (all captured outcomes)",
            &[
                ("Track", "track"),
                ("Arm", "arm"),
                ("Known totals", "known_totals"),
                ("Unknown totals", "unknown_totals"),
                ("Actual step values", "values"),
                ("Median", "median"),
                ("Range", "range"),
            ][..],
        ),
        (
            "step_distributions",
            "Actual steps by complexity (all captured outcomes)",
            &[
                ("Complexity", "complexity"),
                ("Arm", "arm"),
                ("Known totals", "known_totals"),
                ("Unknown totals", "unknown_totals"),
                ("Actual step values", "values"),
                ("Median", "median"),
                ("Range", "range"),
                ("Design target", "design_target"),
                ("Within target", "within_target"),
            ][..],
        ),
        (
            "repeat_comparisons",
            "Repetitions of the same fixed task",
            &[
                ("Track", "track"),
                ("Task", "task_id"),
                ("Arm", "arm"),
                ("Repeat evidence", "repetitions"),
                ("Second minus first steps", "first_two_step_difference"),
                ("Same status", "first_two_status_equal"),
            ][..],
        ),
    ] {
        let Some(rows) = model[key].as_array().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        append_table(
            &mut output,
            title,
            &fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            rows.iter()
                .map(|row| {
                    fields
                        .iter()
                        .map(|(_, key)| {
                            let value = if *key == "track" && row[*key].is_null() {
                                &model["pairs"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .find(|pair| pair["task_id"] == row["task_id"])
                                    .unwrap_or(&Value::Null)["track"]
                            } else {
                                &row[*key]
                            };
                            super::cell(value)
                        })
                        .collect()
                })
                .collect(),
        );
    }
    if !output.is_empty() {
        output.push_str("Targets are calibration goals, not minimum steps. Unknown totals retain their observed prefixes in JSON/HTML. Basic capabilities and complexity workflows are separate populations; no pooled advantage is inferred. Repetitions are nested within tasks; differences do not establish their cause. Interaction activities can overlap within a decision, and a first successful compile is not task completion or a repair-step count. Runtime denial messages are observed stderr diagnostics, not a complete or authenticated child-process audit.\n");
    }
    output
}

fn append_table(output: &mut String, title: &str, headers: &[&str], rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        return;
    }
    output.push_str(&format!(
        "## {title}\n\n| {} |\n| {} |\n",
        headers.join(" | "),
        vec!["---"; headers.len()].join(" | ")
    ));
    for row in rows {
        let cells: Vec<_> = row
            .iter()
            .map(|value| value.replace('|', "\\|").replace(['\r', '\n'], " "))
            .collect();
        output.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    output.push('\n');
}
