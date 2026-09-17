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
    let mut trace_events = Vec::new();
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
            let Ok(text) = fs::read_to_string(bundle.join("trace.jsonl")) else {
                continue;
            };
            let events: Vec<Value> = text
                .lines()
                .filter_map(|s| serde_json::from_str(s).ok())
                .collect();
            let mut starts = std::collections::BTreeMap::new();
            for event in events {
                if let (Some(time), Some(domain)) = (
                    event["monotonic_ns"].as_u64(),
                    event["clock_domain"].as_str(),
                ) {
                    trace_events.push(json!({"name":event["payload"]["type"],"ph":"i","s":"t","pid":id,"tid":domain,"ts":time as f64/1000.0,"args":{"source_seq":event["seq"],"payload":event["payload"],"confidence":"observed"}}));
                }
                let observation = &event["payload"]["observation"];
                if observation["type"] == "started" {
                    starts.insert(observation["step_id"].clone().to_string(), event.clone());
                }
                if observation["type"] == "finished"
                    && let Some(start) = starts.get(&observation["step_id"].to_string())
                {
                    let (Some(begin), Some(end)) = (
                        start["wall_time_unix_ms"].as_u64(),
                        event["wall_time_unix_ms"].as_u64(),
                    ) else {
                        continue;
                    };
                    let span = digest(format!("{id}:{}", observation["step_id"]).as_bytes());
                    spans.push(json!({"traceId":trace_id,"spanId":&span[..16],"parentSpanId":root_span,"name":"codex.agent_step","kind":1,"startTimeUnixNano":(begin*1_000_000).to_string(),"endTimeUnixNano":(end*1_000_000).to_string(),"attributes":[attribute("mbtx.step_id",&observation["step_id"]),attribute("mbtx.outcome",&observation["outcome"]),attribute("mbtx.source_seq",&event["seq"]),attribute("mbtx.confidence",&json!("observed"))]}));
                }
            }
        }
    }
    json_new(
        &output.join("trace.json"),
        &json!({"traceEvents":trace_events,"displayTimeUnit":"ms","metadata":{"clock_rule":"Each thread has its own monotonic epoch. Cross-domain absolute placement is not aligned.","missing_timestamps":"Omitted from the projection, retained in raw events"}}),
    )?;
    json_new(
        &output.join("steps.otlp.json"),
        &json!({"resourceSpans":[{"resource":{"attributes":[attribute("service.name",&json!("mbtx-evaluation"))]},"scopeSpans":[{"scope":{"name":"mbtx-native-trace-projection","version":"1"},"spans":spans}]}]}),
    )
}
