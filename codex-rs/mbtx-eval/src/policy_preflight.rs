//! Actual upstream MoonRun admission checks, completed before relay probes.
use std::fs;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::json;

pub(crate) async fn check(execution: &crate::attempt::Execution<'_>) -> Result<()> {
    let id = format!("preflight-policy-{}", uuid::Uuid::new_v4());
    let evidence = execution.root.join(&id);
    let work = execution.root.join("workspaces").join(&id);
    fs::create_dir(&evidence)?;
    for name in ["workspace", "home", "tmp"] {
        fs::create_dir_all(work.join(name))?;
    }
    fs::copy(
        execution.bundle.join("fixture-worker"),
        work.join("workspace/fixture-worker"),
    )?;
    fs::copy(
        execution.bundle.join("process-policy.mbtx"),
        work.join("workspace/policy-check.mbtx"),
    )?;
    let receipts = crate::worker_receipts::WorkerReceipts::start(
        &evidence,
        &execution.bundle.join("fixture-worker"),
        &work.join("workspace"),
    )
    .await?;
    crate::evidence::json_new(&work.join("worker-socket.json"), &json!(receipts.socket))?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = crate::submission::Validator {
        bundle: execution.bundle,
        bundle_info: execution.bundle_info,
        path: execution.manifest["path"].as_str().context("PATH")?,
        utilities: &execution.manifest["utilities"],
        cancellation: &cancellation,
    };
    let result =
        async {
            let task = execution.manifest["protocol"]["tasks"]
                .as_array()
                .and_then(|tasks| tasks.first())
                .context("policy preflight task")?;
            let natural = execution.manifest["protocol"]["suite"] == "natural";
            if let Some(profile) = task["process_policy_profile"].as_str() {
                ensure!(
                    profile
                        == if natural {
                            "natural-direct-process-v1"
                        } else {
                            "v4-direct-process-v1"
                        },
                    "policy profile does not match protocol"
                );
            }
            let mut allow = task["process_allow"]
                .as_array()
                .context("policy preflight process allow")?
                .clone();
            for action in [
                "square",
                "checked-square",
                "echo",
                "recover",
                "job",
                "emit",
                "hash",
            ] {
                let rule = json!({"program":"fixture-worker","args_prefix":[action]});
                if !allow.iter().any(|existing| existing == &rule) {
                    allow.push(rule);
                }
            }
            // Keep this count beside the fixture's exact denied request list.
            // It is recorded below so a changed preflight fixture cannot be
            // mistaken for a successful policy check.
            let expected_denials = if natural { 12 } else { 13 };
            let policy = crate::process_policy::prepare(
                &json!({
                    "process_allow":allow,
                    "process_policy_profile":task["process_policy_profile"],
                    "cohort":task["cohort"]
                }),
                &work,
                &evidence,
                execution.bundle,
                validator.utilities,
            )?
            .context("policy")?;
            // Frozen dependency sources are copied only for preflight compilation.
            for entry in walkdir::WalkDir::new(execution.bundle.join("dependencies")) {
                let entry = entry?;
                let destination = work.join("workspace/dependencies").join(
                    entry
                        .path()
                        .strip_prefix(execution.bundle.join("dependencies"))?,
                );
                if entry.file_type().is_dir() {
                    fs::create_dir_all(destination)?;
                } else {
                    ensure!(entry.file_type().is_file(), "preflight dependency symlink");
                    fs::copy(entry.path(), destination)?;
                }
            }
            let mut compiler = validator.command(&work, None)?;
            compiler
                .arg(
                    execution.bundle_info["moon_path"]
                        .as_str()
                        .context("moon")?,
                )
                .args([
                    "build",
                    "--frozen",
                    "--target",
                    "wasm",
                    "--release",
                    "--target-dir",
                    "build",
                    "policy-check.mbtx",
                ]);
            let build = crate::validation_process::capture(
                &mut compiler,
                &evidence.join("build"),
                Duration::from_secs(120),
            )
            .await?;
            ensure!(
                build["exit_code"] == 0 && build["timed_out"] == false,
                "policy fixture compilation failed"
            );
            // command() writes its config exclusively; this separate invocation reuses
            // the same immutable config and complete sandbox argv from compilation.
            let mut runtime = tokio::process::Command::new(execution.bundle.join("codex"));
            runtime
                .args(["sandbox", "--allow-unix-socket"])
                .arg(&receipts.socket)
                .args(["--permission-profile", "evaluation", "-C"])
                .arg(work.join("workspace"))
                .arg("--")
                .arg(work.join("workspace/fixture-worker"))
                .arg("launch")
                .arg(work.join("tmp/runtime-entered"))
                .arg(
                    execution.bundle_info["moonrun_path"]
                        .as_str()
                        .context("moonrun")?,
                )
                .arg("--policy")
                .arg(&policy.path)
                .arg("--")
                .arg(work.join(
                    "workspace/build/policy-check.mbtx/wasm/release/build/single/single.wasm",
                ))
                .args(if natural {
                    vec!["--natural"]
                } else {
                    Vec::new()
                })
                .current_dir(work.join("workspace"))
                .env_clear()
                .env("CODEX_HOME", work.join("codex-home"))
                .env("HOME", work.join("home"))
                .env("TMPDIR", work.join("tmp"))
                .env("PATH", &policy.execution_path)
                .env("MBTX_WORKER_SOCKET", &receipts.socket);
            let process = crate::validation_process::capture(
                &mut runtime,
                &evidence.join("execution"),
                Duration::from_secs(120),
            )
            .await?;
            ensure!(
                process["exit_code"] == 0
                    && process["timed_out"] == false
                    && process["drain_complete"] == true
                    && process["residual_group_before_cleanup"] == false
                    && work.join("tmp/runtime-entered").is_file(),
                "policy runtime preflight failed"
            );
            let stderr = fs::read_to_string(evidence.join("execution/stderr"))?;
            ensure!(
                stderr
                    .lines()
                    .filter(|line| *line == "Sandbox policy blocked process spawn")
                    .count()
                    == expected_denials,
                "unexpected native direct-spawn rejection diagnostics"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
    let finished = receipts.finish().await;
    let result = result.and(finished).and_then(|()| {
        let events = crate::evidence::worker_events(&evidence)?;
        let events = events.as_array().context("receipt array")?;
        ensure!(
            events.len() == 25
                && events.iter().all(|e| e["confidence"] == "observed")
                && events.iter().filter(|e| e["event"]["phase"] == "started").count() == 11
                && events.iter().filter(|e| e["event"]["phase"] == "completed").count() == 11
                && events.iter().filter(|e| e["event"]["phase"] == "operation").count() == 3,
            "expected eleven authenticated worker processes and three validated state operations; help and malformed arguments must remain diagnostics"
        );
        Ok(())
    });
    crate::evidence::json_new(
        &evidence.join("result.json"),
        &json!({"status":if result.is_ok(){"success"}else{"local_preflight_failure"},"error":result.as_ref().err().map(ToString::to_string),"scope":"upstream direct-spawn admission and worker IPC before any relay request; descendant confinement not claimed","expected_denial_diagnostics":if execution.manifest["protocol"]["suite"] == "natural" {12} else {13}}),
    )?;
    crate::evidence::seal(&evidence)?;
    result.with_context(|| {
        format!(
            "policy preflight failed before API requests; inspect {}",
            evidence.display()
        )
    })?;
    eprintln!("[eval] upstream MoonRun policy and direct-process preflight passed");
    Ok(())
}
