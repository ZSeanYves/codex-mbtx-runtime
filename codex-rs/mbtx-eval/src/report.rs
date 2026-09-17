use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::analysis::Analysis;
use crate::evidence::json_new;
use crate::evidence::read_json;
use crate::evidence::verify;
use crate::evidence::write_new;

pub(crate) fn response_summary(body: &str) -> (Option<&'static str>, Option<String>) {
    let mut data = String::new();
    let mut terminal = None;
    let mut response_id = None;
    for line in body.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                if let Some(id) = value["response"]["id"].as_str() {
                    response_id = Some(id.to_owned());
                }
                match value["type"].as_str() {
                    Some("response.completed") => terminal = Some("completed"),
                    Some("response.failed" | "error") => terminal = Some("failed"),
                    _ => (),
                }
            }
            data.clear();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.trim_start());
            data.push('\n');
        }
    }
    (terminal, response_id)
}

pub(crate) fn find_attempt(root: &Path, pair: &Value, arm: &str) -> Result<PathBuf> {
    for directory in directories(&root.join("attempts"))? {
        let assignment = read_json(&directory.join("assignment.json"))?;
        if assignment["pair_id"] == *pair && assignment["arm"] == arm {
            return Ok(directory);
        }
    }
    anyhow::bail!("no recorded attempt for {pair} {arm}")
}

pub(crate) fn directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(root)?
        .map(|v| v.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.is_dir());
    paths.sort();
    Ok(paths)
}

pub(crate) async fn assess_attempt(
    directory: &Path,
    manifest: &Value,
    analysis: &mut Analysis,
) -> Result<Value> {
    let sealed = directory.join("seal.json").exists();
    let verified = sealed && verify(directory).unwrap_or(false);
    let cache = if verified && manifest["manifest_integrity"] != false {
        let key = crate::evidence::digest(&[fs::read(directory.join("seal.json"))?,serde_json::to_vec(&manifest["protocol"])?,analysis.fingerprint.as_bytes().to_vec()].concat());
        let root = directory.parent().and_then(Path::parent).context("run root")?.join("derived-analysis");
        fs::create_dir_all(&root)?;
        Some(root.join(format!("{key}.json")))
    } else { None };
    if let Some(path) = &cache && let Ok(cached) = read_json(path)
        && cached["sha256"] == crate::evidence::digest(&serde_json::to_vec(&cached["analysis"])?) {
        return Ok(cached["analysis"].clone());
    }
    let assignment = read_json(&directory.join("assignment.json")).unwrap_or(Value::Null);
    let attempt_id = directory
        .file_name()
        .context("attempt directory name")?
        .to_string_lossy();
    let task = manifest["protocol"]["tasks"]
        .as_array()
        .context("tasks")?
        .iter()
        .find(|t| t["id"] == assignment["task_id"])
        .cloned()
        .unwrap_or(Value::Null);
    let mut errors = Vec::new();
    let mut tool_results = serde_json::Map::new();
    let assignment_valid = assignment["attempt_id"] == attempt_id.as_ref()
        && matches!(
            assignment["arm"].as_str(),
            Some("shell_tool" | "mbtx_program")
        )
        && manifest["schedule"].as_array().is_some_and(|pairs| {
            pairs.iter().any(|p| {
                p["pair_id"] == assignment["pair_id"] && p["task_id"] == assignment["task_id"]
            })
        })
        && !task.is_null();
    if !assignment_valid {
        errors
            .push("assignment absent, incomplete, or inconsistent with the frozen schedule".into());
    }
    let mut trace = Value::Null;
    if directory.join("trace").exists() {
        let bundles = directories(&directory.join("trace"))?;
        if bundles.len() == 1 {
            match codex_rollout_trace::replay_bundle(&bundles[0]) {
                Ok(value) => {
                    trace = serde_json::to_value(value)?;
                    tool_results = crate::report_details::tool_results(&bundles[0], &trace)
                        .as_object().cloned().unwrap_or_default();
                }
                Err(error) => errors.push(format!("native reduction: {error}")),
            }
        } else {
            errors.push(format!(
                "expected one native capture, found {}",
                bundles.len()
            ));
        }
    }
    let outcome = read_json(&directory.join("outcome.json")).unwrap_or(Value::Null);
    let snapshot = read_json(&directory.join("snapshot.json"))
        .unwrap_or_else(|_| json!({"files":{},"errors":["snapshot absent or incomplete"]}));
    let mut exchanges = Vec::new();
    if directory.join("http").exists() {
        for request in directories(&directory.join("http"))? {
            let mut value = read_json(&request.join("result.json")).unwrap_or_else(
                |_| json!({"complete":false,"status_code":null,"transport_error":null}),
            );
            value["started"] = read_json(&request.join("started.json")).unwrap_or(Value::Null);
            value["headers"] = read_json(&request.join("headers.json")).unwrap_or(Value::Null);
            let sse = value["headers"]["content_type"]
                .as_str()
                .is_some_and(|v| v.starts_with("text/event-stream"));
            let (terminal, response_id) = if sse {
                fs::read_to_string(request.join("response.body")).ok().map(|body|response_summary(&body)).unwrap_or_default()
            } else { (None,None) };
            value["sse_terminal"] = json!(terminal);
            value["response_id"] = json!(response_id);
            value["sse_expected"] = json!(sse);
            value["evidence"] = json!(request.strip_prefix(directory)?.to_string_lossy());
            exchanges.push(value);
        }
    }
    let integrity = if !assignment_valid || manifest["manifest_integrity"] == false {
        json!(false)
    } else if directory.join("seal.json").exists() {
        json!(verified)
    } else {
        Value::Null
    };
    let facts = json!({"attempt_id":attempt_id,"pair_id":assignment["pair_id"],"arm":assignment["arm"],"trace":trace,"tool_results":tool_results,"snapshot":snapshot,"outcome":outcome,"workspace":read_json(&directory.join("workspace.json")).unwrap_or(Value::Null),"postprocess":read_json(&directory.join("postprocess.json")).unwrap_or(Value::Null),"submission":read_json(&directory.join("submission.json")).unwrap_or(Value::Null),"execution_error":read_json(&directory.join("execution-error.json")).unwrap_or(Value::Null),"exchanges":exchanges,"integrity":integrity,"evidence":{"directory":format!("attempts/{attempt_id}"),"errors":errors}});
    let result = analysis
        .query(json!({"op":"attempt","task":task,"facts":facts}))
        .await?;
    if let Some(path) = cache && !path.exists() {
        // A partial cache file is never accepted: parsing and its own hash must
        // both pass. Raw evidence is still verified before every reuse.
        json_new(&path,&json!({"sha256":crate::evidence::digest(&serde_json::to_vec(&result)?),"analysis":result}))?;
    }
    Ok(result)
}

pub(crate) async fn assess(root: &Path, analysis: &mut Analysis) -> Result<Vec<Value>> {
    let mut manifest = read_json(&root.join("run.json"))?;
    manifest["manifest_integrity"] = json!(crate::evidence::verify_manifest(root).unwrap_or(false));
    let mut assigned = Vec::new();
    for directory in directories(&root.join("attempts"))? {
        let meta = read_json(&directory.join("assignment.json")).unwrap_or(Value::Null);
        assigned.push((meta["assigned_ms"].as_u64().unwrap_or(0), directory));
    }
    assigned.sort();
    let mut result = Vec::new();
    for (_, directory) in assigned {
        result.push(assess_attempt(&directory, &manifest, analysis).await?);
    }
    Ok(result)
}

fn cell(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn render(model: &Value, format: &str) -> Result<String> {
    if format == "html" { return crate::report_html::render(model); }
    if format == "json" {
        return Ok(serde_json::to_string_pretty(model)?);
    }
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
            cell(&model["pairs"].as_array().into_iter().flatten().find(|p|p["pair_id"]==a["pair_id"]).unwrap_or(&Value::Null)["repeat"]),
            cell(&a["steps_to_success"]),
        ]);
    }
    for pair in model["pairs"].as_array().into_iter().flatten() {
        for arm in ["shell_tool","mbtx_program"] {
            if model["attempts"].as_array().into_iter().flatten().any(|a|a["pair_id"]==pair["pair_id"]&&a["arm"]==arm){continue;}
            let mut row=vec!["null".to_owned();rows[0].len()];
            for (index,value) in [(0,cell(&pair["task_id"])),(1,cell(&pair["scenario"])),(2,cell(&pair["variant"])),(3,arm.into()),(4,"not_started".into()),(17,cell(&pair["cohort"])),(18,cell(&pair["complexity"])),(19,cell(&pair["pair_id"])),(20,cell(&pair["repeat"]))]{row[index]=value;}
            rows.push(row);
        }
    }
    if format == "csv" {
        return Ok(rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| format!("\"{}\"", v.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n");
    }
    let summary = format!(
        "Run {} | {} | {} | partial={}\nShell: {} successful / {} assigned. MBTX: {} successful / {} assigned.\nComparable successful pairs: {}. Mean MBTX minus Shell steps: {}.\nFixed replay is validation only. Failed and unstarted arms remain in assigned denominators; they have no steps-to-success value. The conditional mean and interval do not establish an unconditional advantage when success differs. Program-delivery success requires fresh-input validation; permitted native utilities remain part of the treatment. Timing is diagnostic and includes observation overhead. Unknown values remain null.",
        cell(&model["run_id"]),
        cell(&model["mode"]),
        cell(&model["platform"]),
        model["partial"],
        model["itt"]["shell_tool"]["successes"],
        model["itt"]["shell_tool"]["assigned"],
        model["itt"]["mbtx_program"]["successes"],
        model["itt"]["mbtx_program"]["assigned"],
        model["conditional"]["pairs"],
        model["conditional"]["mean_step_difference"]
    );
    let population = format!(
        "Protocol: {}. Suite: {}. Conditional 95% interval: {}. {}",
        cell(&model["protocol_id"]),
        cell(&model["suite"]),
        cell(&model["conditional"]["confidence_interval"]),
        cell(&model["conditional"]["interval_reason"])
    );
    let mut strata = String::from(
        "| Scenario | Shell successful / assigned | MBTX successful / assigned | MBTX minus Shell steps | Conditional 95% interval |\n|---|---:|---:|---:|---|\n",
    );
    for row in model["by_scenario"].as_array().into_iter().flatten() {
        let cells = [
            cell(&row["name"]),
            format!(
                "{} / {}",
                row["itt"]["shell_tool"]["successes"], row["itt"]["shell_tool"]["assigned"]
            ),
            format!(
                "{} / {}",
                row["itt"]["mbtx_program"]["successes"], row["itt"]["mbtx_program"]["assigned"]
            ),
            cell(&row["conditional"]["mean_step_difference"]),
            cell(&row["conditional"]["confidence_interval"]),
        ];
        strata.push_str(&format!(
            "| {} |\n",
            cells
                .iter()
                .map(|v| v.replace('|', "\\|").replace(['\r', '\n'], " "))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    if format == "md" {
        let mut cohorts=String::from("| Cohort | Comparable pairs | Mean step difference | 95% interval | Mean step ratio | Ratio 95% interval |\n|---|---:|---:|---|---:|---|\n");
        for group in model["by_cohort"].as_array().into_iter().flatten(){
            let e=&group["conditional"];
            cohorts.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",cell(&group["name"]),e["pairs"],e["mean_step_difference"],e["confidence_interval"],e["mean_step_ratio"],e["ratio_confidence_interval"]));
        }
        let table = rows
            .iter()
            .map(|row| {
                format!(
                    "| {} |",
                    row.iter()
                        .map(|v| v.replace('|', "\\|").replace(['\r', '\n'], " "))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            })
            .collect::<Vec<_>>();
        return Ok(format!(
            "# Programmable MBTX evaluation report\n\n{summary}\n\n{population}\n\n{cohorts}\n\n{strata}\n\n## Attempt evidence\n\n{}\n|{}|\n{}\n\nAll recorded decisions, payloads and full output resources are embedded in report.html and report.json. Ordinal trajectory differences are not semantic alignment or causal attribution. Success-conditioned estimates may be selected by differential failure; repetitions are nested within input cases. Native evidence hashes and the frozen protocol are retained in the report method and per-attempt detail records.\n",
            table[0],
            vec!["---"; rows[0].len()].join("|"),
            table[1..].join("\n")
        ));
    }
    anyhow::bail!("unsupported report format: {format}")
}

pub(crate) async fn generate(
    root: &Path,
    bundle: &Path,
    analysis: &mut Analysis,
    format: &str,
) -> Result<PathBuf> {
    let generation_started = std::time::Instant::now();
    let mut manifest = read_json(&root.join("run.json"))?;
    manifest["manifest_integrity"] = json!(crate::evidence::verify_manifest(root).unwrap_or(false));
    let attempts = assess(root, analysis).await?;
    let latest = fs::read_dir(root)?
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("collection-session-")
        })
        .filter_map(|entry| read_json(&entry.path()).ok())
        .max_by_key(|value| value["ended_ms"].as_u64());
    manifest["collection_stop_reason"] = latest
        .map(|v| v["stop_reason"].clone())
        .unwrap_or(Value::Null);
    let mut model = analysis
        .query(json!({"op":"report","manifest":manifest,"attempts":attempts}))
        .await?;
    model["method"] = json!({"manifest":manifest,"analyzer_bundle":read_json(&bundle.join("bundle.json"))?,"rendering":"ECharts 6.0.0; all data and assets embedded; no network requests"});
    for attempt in model["attempts"].as_array_mut().context("attempts")? {
        let directory = root.join("attempts").join(crate::evidence::safe_relative(attempt["attempt_id"].as_str().context("attempt id")?)?);
        attempt["details"] = crate::report_details::collect(&directory)?;
    }
    let analysis_ns = generation_started.elapsed().as_nanos();
    let output = root.join("reports").join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&output)?;
    json_new(
        &output.join("analysis-input.json"),
        &json!({"manifest":manifest,"attempts":attempts,"analyzer_bundle":read_json(&bundle.join("bundle.json"))?}),
    )?;
    let formats = if format == "all" {
        vec!["json", "csv", "md", "html"]
    } else if format == "json" {
        vec![format]
    } else {
        vec!["json", format]
    };
    for format in formats {
        write_new(
            &output.join(format!("report.{format}")),
            render(&model, format)?.as_bytes(),
        )?;
    }
    crate::observe::export_file(root, &output, &model)?;
    json_new(&output.join("generation.json"), &json!({"analysis_and_evidence_read_ns":analysis_ns,"render_write_and_trace_export_ns":generation_started.elapsed().as_nanos()-analysis_ns,"scope":"post-execution report preparation; excludes final report seal; not included in attempt measurements"}))?;
    crate::evidence::seal(&output)?;
    eprintln!("[eval] report: {}", output.display());
    Ok(output)
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
