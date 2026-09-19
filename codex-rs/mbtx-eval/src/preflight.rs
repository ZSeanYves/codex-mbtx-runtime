//! Verify sandbox entry and authenticated worker receipts before any API call.
//! This uses an independent workspace and never enters task-step accounting.
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::json;

use crate::evidence::digest;
use crate::evidence::json_new;
use crate::evidence::seal;
use crate::submission::Validator;

pub(crate) async fn check(root: &Path, validator: &Validator<'_>) -> Result<()> {
    let name = format!("preflight-{}", uuid::Uuid::new_v4());
    let evidence = root.join(&name);
    let work = root.join("workspaces").join(&name);
    fs::create_dir(&evidence)?;
    for directory in ["workspace", "home", "tmp"] {
        fs::create_dir_all(work.join(directory))?;
    }
    let worker = work.join("workspace/fixture-worker");
    fs::copy(validator.bundle.join("fixture-worker"), &worker)?;
    let input = b"local sandbox and receipt preflight\n";
    let path = work.join("workspace/input.txt");
    fs::write(&path, input)?;
    let receipts = crate::worker_receipts::WorkerReceipts::start(&evidence, &worker).await?;
    json_new(&work.join("worker-socket.json"), &json!(receipts.socket))?;
    let result = async {
        let mut command = validator.command(&work, /*readable*/ None)?;
        command.arg(&worker).arg("hash").arg(&path);
        let process = crate::validation_process::capture(
            &mut command,
            &evidence.join("execution"),
            Duration::from_secs(30),
        )
        .await?;
        ensure!(
            process["exit_code"] == 0
                && process["timed_out"] == false
                && process["drain_complete"] == true
                && process["residual_group_before_cleanup"] == false
                && work.join("tmp/entry-observed").is_file(),
            "sandbox or worker command did not complete cleanly"
        );
        ensure!(
            fs::read_to_string(evidence.join("execution/stdout"))?.trim() == digest(input),
            "sandboxed worker could not read its private input"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let finished = receipts.finish().await;
    let result = result.and(finished).and_then(|()| {
        let facts = crate::evidence::worker_events(&evidence)?;
        let events = facts.as_array().context("worker event records")?;
        ensure!(
            events.len() == 2
                && events[0]["event"]["phase"] == "started"
                && events[1]["event"]["phase"] == "completed"
                && events[1]["event"]["exit_code"] == 0
                && events.iter().all(|event| event["confidence"] == "observed"),
            "sandboxed worker receipts were not authenticated"
        );
        Ok(())
    });
    json_new(
        &evidence.join("result.json"),
        &json!({
            "status":if result.is_ok(){"success"}else{"local_preflight_failure"},
            "error":result.as_ref().err().map(ToString::to_string),
            "sandbox_entry_observed":work.join("tmp/entry-observed").is_file(),
            "failure_layer":if result.is_ok(){None}else if !work.join("tmp/entry-observed").is_file(){Some("sandbox/launcher")}else{Some("worker IPC/execution")},
            "failure_attribution":"inferred from the observed preflight stage; raw stderr remains authoritative",
            "scope":"Local sandbox and worker IPC only; before relay probes and task attempts",
        }),
    )?;
    seal(&evidence)?;
    result.with_context(|| format!(
        "sandbox preflight failed before API requests; inspect {}/execution/stderr and result.json",
        evidence.display()
    ))?;
    eprintln!("[eval] local sandbox and worker receipt preflight passed");
    Ok(())
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
