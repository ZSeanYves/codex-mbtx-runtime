//! Lossless report materialization, after execution. No tool or task decisions.
use crate::evidence::digest;
use crate::evidence::read_json;
use crate::evidence::safe_relative;
use anyhow::Context;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::Path;

fn payload(bundle: &Path, trace: &Value, id: &Value) -> Result<Value> {
    let Some(id) = id.as_str() else {
        return Ok(Value::Null);
    };
    let relative = trace["raw_payloads"][id]["path"]
        .as_str()
        .context("payload path")?;
    let path = bundle.join(safe_relative(relative)?);
    anyhow::ensure!(
        path.canonicalize()?.starts_with(bundle.canonicalize()?),
        "payload escaped trace"
    );
    read_json(&path)
}

pub(crate) fn tool_results(bundle: &Path, trace: &Value) -> Value {
    let mut results = serde_json::Map::new();
    for (id, tool) in trace["tool_calls"].as_object().into_iter().flatten() {
        let invocation = payload(bundle, trace, &tool["raw_invocation_payload_id"]);
        let result = payload(bundle, trace, &tool["raw_result_payload_id"]);
        let value = match (invocation, result) {
            (Ok(invocation), Ok(result)) => {
                let arguments = invocation["payload"]["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or(Value::Null);
                let output = result["response_item"]["output"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or(Value::Null);
                json!({"arguments":arguments,"output":output,"invocation":invocation,"model_visible_result":result,"evidence":tool["raw_result_payload_id"]})
            }
            (a, b) => {
                json!({"evidence_error":format!("invocation: {:?}; result: {:?}",a.err(),b.err())})
            }
        };
        results.insert(id.clone(), value);
    }
    Value::Object(results)
}

pub(crate) fn collect(directory: &Path) -> Result<Value> {
    let mut errors = Vec::new();
    let mut payloads = serde_json::Map::new();
    let mut events = Vec::new();
    for bundle in crate::report::directories(&directory.join("trace")).unwrap_or_default() {
        match codex_rollout_trace::replay_bundle(&bundle) {
            Ok(trace) => {
                let trace = serde_json::to_value(trace)?;
                for id in trace["raw_payloads"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(id, _)| id)
                {
                    match payload(&bundle, &trace, &json!(id)) {
                        Ok(value) => {
                            payloads.insert(id.clone(), value);
                        }
                        Err(error) => errors.push(format!("{id}: {error}")),
                    }
                }
            }
            Err(error) => errors.push(format!("trace reduction: {error}")),
        }
        if let Ok(text) = fs::read_to_string(bundle.join("trace.jsonl")) {
            for line in text.lines() {
                match serde_json::from_str::<Value>(line) {
                    Ok(event) => events.push(event),
                    Err(error) => errors.push(format!("partial trace line: {error}")),
                }
            }
        }
    }
    let mut resources = Vec::new();
    let root = directory.join("resources");
    if root.exists() {
        for entry in walkdir::WalkDir::new(&root).follow_links(false) {
            let entry = entry?;
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|s| s != "json")
            {
                continue;
            }
            let mut receipt = match read_json(entry.path()) {
                Ok(value) => value,
                Err(error) => {
                    errors.push(format!(
                        "partial resource {}: {error}",
                        entry.path().strip_prefix(directory)?.display()
                    ));
                    continue;
                }
            };
            let data = entry.path().with_extension("data");
            match fs::read(&data) {
                Ok(bytes) => {
                    receipt["archived_bytes"] = json!(bytes.len());
                    receipt["archived_sha256"] = json!(digest(&bytes));
                    receipt["hash_verified"] = if receipt["sha256"].is_string() {
                        json!(receipt["sha256"] == receipt["archived_sha256"])
                    } else {
                        Value::Null
                    };
                    if receipt["phase"] != "artifact" {
                        receipt["data_base64"] = json!(STANDARD.encode(bytes));
                    } else {
                        receipt["binary_evidence"] = json!(data.strip_prefix(directory)?);
                    }
                }
                Err(error) => {
                    receipt["read_error"] = json!(error.to_string());
                }
            }
            receipt["evidence"] = json!(entry.path().strip_prefix(directory)?);
            resources.push(receipt);
        }
    }
    resources.sort_by(|a, b| a["resource_id"].as_str().cmp(&b["resource_id"].as_str()));
    Ok(
        json!({"raw_payloads":payloads,"events":events,"resources":resources,"errors":errors,
        "seal":read_json(&directory.join("seal.json")).unwrap_or(Value::Null),
        "process":read_json(&directory.join("process.json")).unwrap_or(Value::Null),
        "worker_events":crate::evidence::worker_events(directory).unwrap_or(Value::Null),
        "scope":"All retained native payloads and output resources; model-private reasoning is not observed. HTTP wire bytes are in the raw evidence archive."}),
    )
}
