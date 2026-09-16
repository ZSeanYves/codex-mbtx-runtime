use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

use crate::evidence::digest;
use crate::evidence::json_new;
use crate::evidence::read_json;

fn attribute(key: &str, value: &Value) -> Value {
    let value = match value {
        Value::Bool(v) => json!({"boolValue":v}),
        Value::Number(v) => json!({"intValue":v.to_string()}),
        _ => {
            json!({"stringValue":value.as_str().map(str::to_owned).unwrap_or_else(||value.to_string())})
        }
    };
    json!({"key":key,"value":value})
}

pub(crate) fn export_file(root: &Path, output: &Path, model: &Value) -> Result<()> {
    let mut spans = Vec::new();
    for attempt in model["attempts"].as_array().context("attempts")? {
        let id = attempt["attempt_id"].as_str().context("attempt id")?;
        let directory = root.join("attempts").join(id);
        let process = read_json(&directory.join("process.json")).unwrap_or(Value::Null);
        let start = process["wall_start_ms"].as_u64();
        let end = attempt["outcome"]["wall_end_ms"].as_u64();
        let trace_id = &digest(id.as_bytes())[..32];
        let root_span = &digest(format!("{id}:attempt").as_bytes())[..16];
        let attributes = vec![
            attribute("mbtx.run_id", &model["run_id"]),
            attribute("mbtx.attempt_id", &attempt["attempt_id"]),
            attribute("mbtx.task", &attempt["task_id"]),
            attribute("mbtx.arm", &attempt["arm"]),
            attribute("mbtx.status", &attempt["status"]),
            attribute(
                "mbtx.agent_steps",
                &attempt["accounting"]["metrics"]["agent_steps"],
            ),
            attribute("mbtx.oracle_success", &attempt["oracle"]["success"]),
            attribute("mbtx.evidence", &attempt["evidence"]["directory"]),
            attribute("mbtx.clock", &json!("producer wall clock; diagnostic only")),
        ];
        if let (Some(start), Some(end)) = (start, end) {
            spans.push(json!({"traceId":trace_id,"spanId":root_span,"name":"mbtx.attempt","kind":1,"startTimeUnixNano":(start*1_000_000).to_string(),"endTimeUnixNano":(end*1_000_000).to_string(),"attributes":attributes}));
        }
        for bundle in crate::report::directories(&directory.join("trace")).unwrap_or_default() {
            let text = fs::read_to_string(bundle.join("trace.jsonl"))?;
            let events: Vec<Value> = text
                .lines()
                .filter_map(|s| serde_json::from_str(s).ok())
                .collect();
            let mut starts = std::collections::BTreeMap::new();
            for event in events {
                let observation = &event["payload"]["observation"];
                if observation["type"] == "started" {
                    starts.insert(observation["step_id"].clone().to_string(), event.clone());
                }
                if observation["type"] == "finished"
                    && let Some(start) = starts.get(&observation["step_id"].to_string())
                {
                    let begin = start["wall_time_unix_ms"]
                        .as_u64()
                        .context("step start time")?;
                    let end = event["wall_time_unix_ms"]
                        .as_u64()
                        .context("step end time")?;
                    let span = digest(format!("{id}:{}", observation["step_id"]).as_bytes());
                    spans.push(json!({"traceId":trace_id,"spanId":&span[..16],"parentSpanId":root_span,"name":"codex.agent_step","kind":1,"startTimeUnixNano":(begin*1_000_000).to_string(),"endTimeUnixNano":(end*1_000_000).to_string(),"attributes":[attribute("mbtx.step_id",&observation["step_id"]),attribute("mbtx.outcome",&observation["outcome"]),attribute("mbtx.source_seq",&event["seq"]),attribute("mbtx.confidence",&json!("observed"))]}));
                }
            }
        }
    }
    json_new(
        &output.join("steps.otlp.json"),
        &json!({"resourceSpans":[{"resource":{"attributes":[attribute("service.name",&json!("mbtx-evaluation"))]},"scopeSpans":[{"scope":{"name":"mbtx-native-trace-projection","version":"1"},"spans":spans}]}]}),
    )
}

pub(crate) async fn import(root: &Path, endpoint: &str) -> Result<usize> {
    let url = reqwest::Url::parse(endpoint)?;
    anyhow::ensure!(
        matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        "telemetry import supports a local collector endpoint only"
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    if root.join("reports").exists() {
        anyhow::ensure!(
            crate::report::directories(&root.join("reports"))?.len() <= 1,
            "multiple report revisions: import attempts/ once and one selected report directory to avoid duplicate spans"
        );
    }
    let mut count = 0;
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let signal = if name == "steps.otlp.json" || name.ends_with("-traces.json") {
            "traces"
        } else if name.ends_with("-logs.json") {
            "logs"
        } else {
            continue;
        };
        let bytes = fs::read(entry.path())?;
        let _: Value = serde_json::from_slice(&bytes).context("OTLP HTTP JSON required")?;
        let response = client
            .post(format!("{}/v1/{signal}", endpoint.trim_end_matches('/')))
            .header("content-type", "application/json")
            .body(bytes)
            .send()
            .await?
            .error_for_status()?;
        let result: Value = response.json().await.context("OTLP collector response")?;
        anyhow::ensure!(
            result.get("partialSuccess").is_none()
                || result["partialSuccess"]
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty),
            "collector reported partial ingestion: {result}"
        );
        count += 1;
    }
    Ok(count)
}
