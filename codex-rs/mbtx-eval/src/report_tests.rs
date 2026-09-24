use super::*;
use pretty_assertions::assert_eq;

#[test]
fn response_completion_requires_an_observed_terminal_event() {
    assert_eq!(
        response_summary(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-1\"}}\n\n"
        ),
        (None, Some("response-1".into()))
    );
    assert_eq!(
        response_summary(
            "event: response.completed\r\ndata: {\"type\":\r\ndata: \"response.completed\"}\r\n\r\n"
        ),
        (Some("completed"), None)
    );
    assert_eq!(
        response_summary("data: {\"type\":\"error\"}\n\n"),
        (Some("failed"), None)
    );
}

#[test]
fn html_escapes_payloads_and_unknown_values_remain_visible() -> Result<()> {
    let model = json!({"run_id":"<script>run</script>","mode":"replay","platform":"test","partial":true,"itt":{},"conditional":{"pairs":0,"mean_step_difference":null},"attempts":[{"task_id":"<img src=x onerror=alert(1)>","arm":"mbtx_program","status":"censored","oracle":{"success":null},"accounting":{"metrics":{"agent_steps":null,"model_requests":null}},"evidence":{"directory":"attempts/id"}}]});
    let html = render(&model, "html")?;
    assert!(!html.contains("<script>run</script>"));
    assert!(!html.contains("<img src=x onerror=alert(1)>"));
    assert!(html.contains("\\u003cimg"));
    assert!(html.contains("null"));
    let csv = render(&model, "csv")?;
    assert!(csv.contains("\"null\""));
    Ok(())
}

#[test]
fn attempt_creation_and_seal_refuse_overwrite_and_detect_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("evidence.json");
    crate::evidence::write_new(&file, b"original")?;
    assert!(crate::evidence::write_new(&file, b"replace").is_err());
    crate::evidence::seal(root.path())?;
    assert!(verify(root.path())?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(&file)?.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&file, permissions)?;
    }
    fs::write(file, b"changed")?;
    assert!(!verify(root.path())?);
    Ok(())
}

#[test]
fn snapshot_rejects_symlink_escape_and_missing_outputs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace)?;
    fs::write(root.path().join("private"), "private")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.path().join("private"), workspace.join("result.json"))?;
    let snapshot =
        crate::evidence::snapshot(&workspace, &json!({"files":{},"output":"result.json"}))?;
    #[cfg(unix)]
    assert_eq!(snapshot["errors"], json!(["result.json"]));
    assert_eq!(snapshot["files"], json!({}));
    Ok(())
}

#[test]
fn html_embeds_every_step_and_original_bytes_without_executable_payload_text() -> Result<()> {
    use base64::Engine;
    use std::io::Read;
    let steps:Vec<_>=(0..512).map(|n|json!({"step_id":format!("step-{n}"),"outcome":"accepted","source":"</script><script>globalThis.injected=true</script>雪"})).collect();
    let attempt = json!({"attempt_id":"large","accounting":{"steps":steps,"metrics":{"agent_steps":512}},"details":{"resources":[{"data_base64":"AP8="}]}});
    let model = json!({"run_id":"</script><script>bad()</script>","attempts":[attempt]});
    let html = render(&model, "html")?;
    assert!(!html.contains("<script>bad()"));
    assert!(!html.contains("<script>globalThis.injected"));
    let marker = "id=\"attempt-0\">";
    let encoded = html
        .split_once(marker)
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let mut decoder = flate2::read::GzDecoder::new(bytes.as_slice());
    let mut decoded = String::new();
    decoder.read_to_string(&mut decoded)?;
    assert_eq!(serde_json::from_str::<Value>(&decoded)?, attempt);
    assert!(html.contains("connect-src 'none'"));
    Ok(())
}

#[test]
fn partially_written_resource_metadata_cannot_prevent_report_reconstruction() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("resources"))?;
    fs::write(
        root.path().join("resources/interrupted.json"),
        b"{\"resource_id\":",
    )?;
    let details = crate::report_details::collect(root.path())?;
    assert_eq!(details["resources"], json!([]));
    assert_eq!(details["errors"].as_array().unwrap().len(), 1);
    Ok(())
}

#[test]
fn offline_report_navigation_and_initial_data_snapshot() -> Result<()> {
    let model = json!({"run_id":"review-example","track_step_distributions":[{"track":"basic","arm":"shell_tool","known_totals":2,"unknown_totals":1,"values":[4,12],"median":8,"range":[4,12]}],"by_track":[{"name":"basic","itt":{"shell_tool":{"captured":1,"assigned":2,"successes":0},"mbtx_program":{"captured":0,"assigned":2,"successes":0}},"conditional":{"pairs":0,"mean_step_difference":null}}],"mode":"replay","partial":true,"step_distributions":[{"complexity":"deep","arm":"mbtx_program","known_totals":0,"unknown_totals":1,"values":[],"median":null,"range":null,"design_target":[50,100],"within_target":0}],"repeat_comparisons":[],"attempts":[{"attempt_id":"one","track":"complexity","interaction":{"first_successful_compile_step":null,"reference_first_pages_observed":1,"reference_continuations_observed":0,"decisions":[{"step":1,"activities":["reference_first_page"]}]},"status":"censored","accounting":{"steps":[{"step_id":"s1"}],"metrics":{"agent_steps_started":1,"agent_steps":null},"coverage":{"steps":false},"complete":false},"tool_outcomes":{"mbtx_compilations_observed":1,"mbtx_program_launches_observed":0,"process_denial_diagnostics_observed":[],"child_process_count":null,"details":{"call-1":{"source":"retained in chunk"}}},"details":{"resources":[]}}]});
    let html = render(&model, "html")?;
    // Snapshot the rendered navigation and initial data. Compressed evidence and
    // vendored JavaScript are covered by the round-trip test, not UI snapshots.
    let body = html
        .split_once("<body>")
        .unwrap()
        .1
        .split_once("<script type=\"application/octet-stream\"")
        .unwrap()
        .0;
    insta::assert_snapshot!(body);
    Ok(())
}

#[test]
fn exported_attempts_keep_outcomes_and_missing_delivery_separate() -> Result<()> {
    let model = json!({
        "attempts":[{"task_id":"delivery","pair_id":"p1","arm":"mbtx_program","status":"task_failure","oracle":{"success":true},"session_terminal":"completed","submission":{"status":"missing_source"},"steps_to_success":null,"tool_outcomes":{"mbtx_compilations_observed":2,"mbtx_program_launches_observed":3,"process_denial_diagnostics_observed":[],"child_process_count":null,"mbtx_result_coverage":true}}],
        "step_distributions":[{"complexity":"deep","arm":"mbtx_program","known_totals":1,"unknown_totals":0,"values":[12],"median":12,"range":[12,12],"design_target":[50,100],"within_target":0}],
        "repeat_comparisons":[{"task_id":"delivery","arm":"mbtx_program","repetitions":[],"first_two_step_difference":null,"first_two_status_equal":null}]
    });
    let csv = render(&model, "csv")?;
    assert!(csv.contains("\"Observed compiler starts\",\"Observed program launches\",\"Observed process denial diagnostics\",\"Child process count\""));
    assert!(csv.contains(
        "\"null\",\"2\",\"3\",\"[]\",\"null\",\"completed\",\"missing_source\",\"true\""
    ));
    let markdown = render(&model, "md")?;
    assert!(
        markdown.contains("| deep | mbtx_program | 1 | 0 | [12] | 12 | [12,12] | [50,100] | 0 |")
    );
    assert!(markdown.contains("## Repetitions of the same fixed task"));
    Ok(())
}

#[test]
fn reports_are_byte_stable_when_object_insertion_order_changes() -> Result<()> {
    let first: Value = serde_json::from_str(
        r#"{"run_id":"stable","pairs":[],"attempts":[{"task_id":"t","accounting":{"metrics":{"agent_steps":7,"tool_calls":4}},"tool_outcomes":{"process_denial_diagnostics_observed":[{"policy_sha256":"abc","diagnostic":"PermissionDenied"}]},"details":{"opaque_future_field":{"z":1,"a":null}}}]}"#,
    )?;
    let second: Value = serde_json::from_str(
        r#"{"attempts":[{"details":{"opaque_future_field":{"a":null,"z":1}},"tool_outcomes":{"process_denial_diagnostics_observed":[{"diagnostic":"PermissionDenied","policy_sha256":"abc"}]},"accounting":{"metrics":{"tool_calls":4,"agent_steps":7}},"task_id":"t"}],"pairs":[],"run_id":"stable"}"#,
    )?;
    assert_eq!(first, second);
    for format in ["json", "html", "md", "csv"] {
        assert_eq!(render(&first, format)?, render(&second, format)?);
        assert_eq!(render(&first, format)?, render(&first, format)?);
    }
    Ok(())
}

#[test]
fn calibration_tables_include_failures_unknown_totals_and_repeat_differences() -> Result<()> {
    let model = json!({
        "run_id":"calibration","attempts":[],
        "track_step_distributions":[{"track":"basic","arm":"shell_tool","captured":3,"known_totals":2,"unknown_totals":1,"values":[9,51],"median":30,"range":[9,51],"observations":[{"status":"task_failure","steps":9,"steps_to_success":null},{"status":"censored","steps":null,"observed_steps":3}]}],
        "step_distributions":[{"complexity":"deep","arm":"shell_tool","captured":3,"known_totals":2,"unknown_totals":1,"values":[9,51],"median":30,"range":[9,51],"design_target":[50,100],"within_target":1,"observations":[{"status":"task_failure","steps":9,"steps_to_success":null},{"status":"censored","steps":null,"observed_steps":3}]}],
        "repeat_comparisons":[{"task_id":"deep-task","arm":"shell_tool","repetitions":[{"repeat":0,"status":"success","steps":51,"steps_to_success":51},{"repeat":1,"status":"task_failure","steps":9,"steps_to_success":null}],"first_two_step_difference":-42,"first_two_status_equal":false}]
    });
    let markdown = render(&model, "md")?;
    assert!(markdown.contains("| basic | shell_tool | 2 | 1 | [9,51] | 30 | [9,51] |"));
    assert!(
        markdown.contains("| deep | shell_tool | 2 | 1 | [9,51] | 30 | [9,51] | [50,100] | 1 |")
    );
    assert!(markdown.contains("\"steps_to_success\":null"));
    assert!(markdown.contains("| -42 | false |"));
    let html = render(&model, "html")?;
    let summary = html
        .split_once("id=\"report-data\">")
        .context("summary")?
        .1
        .split_once("</script>")
        .context("summary close")?
        .0;
    let decoded: Value = serde_json::from_str(summary)?;
    assert_eq!(decoded["step_distributions"], model["step_distributions"]);
    assert_eq!(
        decoded["track_step_distributions"],
        model["track_step_distributions"]
    );
    assert_eq!(decoded["repeat_comparisons"], model["repeat_comparisons"]);
    Ok(())
}

#[test]
fn csv_keeps_stage_counts_separate_from_unknown_child_totals() -> Result<()> {
    let model = json!({"attempts":[{
        "task_id":"task","arm":"mbtx_program","session_terminal":"completed",
        "accounting":{"metrics":{"agent_steps":4}},
        "tool_outcomes":{"mbtx_compilations_observed":2,"mbtx_program_launches_observed":3,"process_denial_diagnostics_observed":[{"diagnostic":"PermissionDenied","policy_sha256":"abc"}],"child_process_count":null,"mbtx_result_coverage":true},
        "submission":{"status":"captured","phase_counts":{"compiler_starts":1,"program_starts":3}}
    }]});
    let csv = render(&model, "csv")?;
    assert!(
        csv.starts_with("\"Task\",\"Scenario\",\"Input variant\",\"Arm\",\"Status\",\"Oracle\"")
    );
    assert!(csv.contains("\"Steps to success\",\"Observed compiler starts\",\"Observed program launches\",\"Observed process denial diagnostics\",\"Child process count\""));
    assert!(csv.contains("\"policy_sha256\"\":\"\"abc\"\""));
    assert!(csv.contains("\"null\",\"completed\",\"captured\",\"true\",\"1\",\"1\",\"3\","));
    let absent = render(&json!({"attempts":[{"task_id":"legacy"}]}), "csv")?;
    assert!(absent.ends_with("\"null\",\"null\",\"null\",\"null\",\"null\",\"null\",\"null\"\r\n"));
    Ok(())
}

#[test]
fn correction_exports_do_not_invent_unselected_assignments() -> Result<()> {
    let model = json!({
        "pairs": [
            {"pair_id":"p0", "assigned_arms":["mbtx_program"]},
            {"pair_id":"p1", "assigned_arms":["mbtx_program"]},
            {"pair_id":"p2", "assigned_arms":[]}
        ],
        "attempts": [{"pair_id":"p0", "arm":"mbtx_program", "status":"success"}]
    });
    let rows = table::rows(&model)?;
    let selected: Vec<Vec<_>> = rows[1..]
        .iter()
        .map(|row| vec![row[19].as_str(), row[3].as_str(), row[4].as_str()])
        .collect();
    assert_eq!(
        selected,
        vec![
            vec!["p0", "mbtx_program", "success"],
            vec!["p1", "mbtx_program", "not_started"],
        ]
    );
    Ok(())
}

#[test]
fn track_exports_keep_unstarted_denominators_and_partial_interaction_observations() -> Result<()> {
    let model = json!({
        "by_track":[{"name":"basic","itt":{"shell_tool":{"assigned":2,"captured":1,"successes":0},"mbtx_program":{"assigned":2,"captured":0,"successes":0}},"conditional":{"pairs":0,"mean_step_difference":null}}],
        "pairs":[{"pair_id":"p1","task_id":"task","track":"basic","repeat":0}],
        "attempts":[{"task_id":"task","pair_id":"p1","track":"basic","arm":"shell_tool","status":"censored","steps_to_success":null,
            "accounting":{"metrics":{"agent_steps":null,"http_transport_retries":1,"agent_stream_retries":null},"observed":{"agent_steps":3,"agent_steps_started":4}},
            "interaction":{"first_successful_compile_step":null,"reference_first_pages_observed":1,"reference_continuations_observed":0,"output_continuations_observed":2,"resource_reads_unknown":0,"decisions":[{"step":1,"activities":["reference_first_page"]}]}
        }]
    });
    let rows = table::rows(&model)?;
    let columns = [
        "Track",
        "Status",
        "Accepted steps",
        "Observed decision prefix",
        "First successful compile decision",
        "Observed reference continuation pages",
        "HTTP transport retries",
        "Agent stream retries",
        "Track assigned denominator",
        "Track successes",
    ];
    let selected: Vec<Vec<_>> = rows[1..]
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|name| row[rows[0].iter().position(|header| header == name).unwrap()].as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        selected,
        vec![
            vec![
                "basic", "censored", "null", "3", "null", "0", "1", "null", "2", "0"
            ],
            vec![
                "basic",
                "not_started",
                "null",
                "null",
                "null",
                "null",
                "null",
                "null",
                "2",
                "0"
            ],
        ]
    );
    let markdown = render(&model, "md")?;
    assert!(markdown.contains("| basic | shell_tool | 1 | 2 | 0 | 0 | null |"));
    assert!(markdown.contains("## Observed interaction costs"));
    let html = render(&model, "html")?;
    let initial: Value = serde_json::from_str(
        html.split_once("id=\"report-data\">")
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )?;
    assert_eq!(initial["by_track"], model["by_track"]);
    assert_eq!(
        initial["attempts"][0]["interaction"]["decisions"],
        Value::Null
    );
    assert_eq!(
        initial["attempts"][0]["interaction"]["reference_first_pages_observed"],
        json!(1)
    );
    Ok(())
}
