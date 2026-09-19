use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use crate::config::RelayConfig;
use crate::config::child_config;
use crate::evidence::json_new;
use crate::evidence::now_ms;
use crate::evidence::safe_relative;
use crate::evidence::seal;
use crate::evidence::snapshot;
use crate::evidence::write_new;
use crate::gate::Gate;
use crate::gate::Route;
use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use tokio::process::Command;

pub(crate) struct Execution<'a> {
    pub root: &'a Path,
    pub bundle: &'a Path,
    pub bundle_info: &'a Value,
    pub config: &'a RelayConfig,
    pub manifest: &'a Value,
    pub gate: &'a Arc<Gate>,
}

impl Execution<'_> {
    pub async fn execute(
        &self,
        task: &Value,
        pair: &Value,
        id: &str,
        route: &Arc<Route>,
    ) -> Result<()> {
        let Self {
            root,
            bundle,
            bundle_info,
            config,
            manifest,
            gate,
        } = self;
        let directory = &route.directory;
        let work = root.join("workspaces").join(id);
        fs::create_dir(&work)?;
        for child in ["workspace", "home", "codex-home", "tmp"] {
            fs::create_dir(work.join(child))?;
        }
        let workspace = work.join("workspace");
        let template = root
            .join("fixtures")
            .join(task["id"].as_str().context("task id")?);
        let prepared = if template.join("baseline.json").exists() {
            Some(crate::workspace::instantiate(&template, &workspace)?)
        } else {
            None
        };
        if prepared.is_none() {
            for path in task["files"].as_object().context("files")?.keys() {
                let file = workspace.join(safe_relative(path)?);
                fs::create_dir_all(file.parent().context("parent")?)?;
                fs::copy(
                    root.join("fixtures")
                        .join(task["id"].as_str().context("task id")?)
                        .join(path),
                    &file,
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
                }
            }
        }
        fs::copy(
            bundle.join("fixture-worker"),
            workspace.join("fixture-worker"),
        )?;
        let workspace_facts = if let Some(facts) = prepared {
            facts
        } else {
            crate::workspace::prepare(
                &workspace,
                &work.join("home"),
                manifest["path"].as_str().context("recorded PATH")?,
            )
            .await?
        };
        json_new(&directory.join("workspace.json"), &workspace_facts)?;
        let receipts = crate::worker_receipts::WorkerReceipts::start(
            directory,
            &bundle.join("fixture-worker"),
        )
        .await?;
        json_new(&work.join("worker-socket.json"), &json!(receipts.socket))?;
        let runtime_policy =
            crate::process_policy::prepare(task, &work, directory, bundle, &manifest["utilities"])?;
        let endpoint = format!("{}/a/{id}/v1", gate.endpoint);
        let instructions = manifest["protocol"]["instructions"]
            .as_str()
            .context("instructions")?;
        // Older frozen protocols have only the common instructions. Never
        // silently give a recorded run a newer submission contract on resume.
        let mut instructions = match manifest["protocol"].get("arm_instructions") {
            Some(arms) if !arms.is_null() => format!(
                "{instructions}\n\n{}",
                arms[&route.arm]
                    .as_str()
                    .context("assigned arm instructions")?
            ),
            _ => instructions.to_owned(),
        };
        if task.get("required_source").is_some()
            && let Some(source) = crate::submission_contract::required_source(task, &route.arm)?
        {
            instructions.push_str(&format!("\nThis task requires the exact saved source {source}. Temporary tool programs and correct visible artifacts do not replace this submission."));
        }
        let max_wall_seconds = manifest["max_wall_seconds"]
            .as_u64()
            .context("recorded attempt wall limit")?;
        let start = Instant::now();
        let cancellation = crate::cancellation::Cancellation::listen()?;
        let wall_start = now_ms().context("attempt wall clock")?;
        let deadline = start + Duration::from_secs(max_wall_seconds);
        let v3 = task.get("process_allow").is_some();
        let mut toml = child_config(
            config,
            bundle,
            bundle_info,
            crate::config::AttemptContext {
                arm: &route.arm,
                endpoint: &endpoint,
                attempt_id: id,
                instructions: &instructions,
                work: &work,
                evidence: directory,
            },
        )?;
        if v3 && route.arm == "mbtx_program" {
            let mut value: toml::Value = toml::from_str(&toml)?;
            let mbtx = value
                .get_mut("mbtx")
                .and_then(toml::Value::as_table_mut)
                .context("MBTX config table")?;
            mbtx.insert(
                "attempt_deadline_unix_ms".into(),
                ((wall_start + max_wall_seconds * 1000) as i64).into(),
            );
            if let Some(policy) = &runtime_policy {
                mbtx.insert(
                    "runtime_policy".into(),
                    policy.path.to_string_lossy().as_ref().into(),
                );
                mbtx.insert(
                    "execution_path".into(),
                    policy.execution_path.as_str().into(),
                );
            }
            toml = toml::to_string_pretty(&value)?;
        }
        if manifest["observation"] == "minimal" {
            let mut value: toml::Value = toml::from_str(&toml)?;
            value["otel"]["exporter"] = "none".into();
            value["otel"]["trace_exporter"] = "none".into();
            toml = toml::to_string_pretty(&value)?;
        }
        write_new(&work.join("codex-home/config.toml"), toml.as_bytes())?;
        write_new(&directory.join("effective-config.toml"), toml.as_bytes())?;
        let mut command = Command::new(bundle.join("codex"));
        command
            .args([
                "exec",
                "--strict-config",
                "--skip-git-repo-check",
                "--ignore-rules",
                "--json",
                "--color",
                "never",
            ])
            .arg("-C")
            .arg(&workspace)
            .arg(task["goal"].as_str().context("goal")?)
            .env_clear()
            .env("PATH", manifest["path"].as_str().context("recorded PATH")?)
            .env("LANG", "C.UTF-8")
            .env("HOME", work.join("home"))
            .env("CODEX_HOME", work.join("codex-home"))
            .env("TMPDIR", work.join("tmp"))
            .env(
                "MOON_TOOLCHAIN_ROOT",
                bundle_info["moon_home"].as_str().context("MoonBit root")?,
            )
            .env("MOON_HOME", bundle.join("moon-home"))
            .env("MBTX_LOCAL_KEY", &route.token)
            .env("NO_PROXY", "127.0.0.1,localhost,::1")
            .env("no_proxy", "127.0.0.1,localhost,::1")
            .env("CODEX_ROLLOUT_TRACE_ROOT", directory.join("trace"))
            .stdin(Stdio::null())
            .stdout(fs::File::create(directory.join("codex.jsonl"))?)
            .stderr(fs::File::create(directory.join("codex.stderr"))?)
            .kill_on_drop(true);
        crate::workspace::git_environment(&mut command, &workspace);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().context("launch bundled Codex")?;
        let pid = child.id().context("Codex PID")?;
        json_new(
            &directory.join("process.json"),
            &json!({"pid":pid,"pgid":pid,"wall_start_ms":wall_start,"deadline_unix_ms":wall_start+max_wall_seconds*1000,"policy_sha256":runtime_policy.as_ref().map(|p|&p.sha256),"clock_domain":format!("attempt-{id}-monotonic")}),
        )?;
        let mut termination = "exited";
        let status = tokio::select! {
            status=child.wait()=>status?,
            _=tokio::time::sleep_until(deadline.into())=> {termination="wall_limit"; terminate(&mut child,pid).await?},
            _=cancellation.cancelled()=> {termination="cancelled"; terminate(&mut child,pid).await?},
        };
        let elapsed = start.elapsed().as_nanos() as u64;
        let wall_end = now_ms();
        let draining = Instant::now();
        route.close();
        tokio::time::timeout(Duration::from_secs(15), gate.drain())
            .await
            .context("upstream stream failed to drain; preserve open attempt")?;
        receipts.finish().await?;
        let collector_drain_ns = draining.elapsed().as_nanos() as u64;
        if route.observation_failures.load(Ordering::SeqCst) > 0 {
            termination = "harness_error";
        }
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal()
        };
        #[cfg(not(unix))]
        let signal: Option<i32> = None;
        let model_termination = termination;
        json_new(
            &directory.join("model-outcome.json"),
            &json!({
                "termination":model_termination,"exit_code":status.code(),"signal":signal,
                "elapsed_ns":elapsed,"wall_end_ms":wall_end,"pid":pid,
                "scope":"Codex process wait; preserved before snapshot and delivery validation"
            }),
        )?;
        let snapshot_started = Instant::now();
        let mut snapshot = snapshot(&workspace, task)?;
        snapshot["worker_events"] = crate::evidence::worker_events(directory)?;
        json_new(&directory.join("snapshot.json"), &snapshot)?;
        let snapshot_ns = snapshot_started.elapsed().as_nanos() as u64;
        let validation_started = Instant::now();
        if cancellation.is_cancelled() {
            termination = "cancelled";
        }
        if task["acceptance"] == "programs" && termination != "cancelled" {
            let validator = crate::submission::Validator {
                bundle,
                bundle_info,
                path: manifest["path"].as_str().context("recorded PATH")?,
                utilities: &manifest["utilities"],
                cancellation: &cancellation,
            };
            let validation = match validator
                .validate(
                    task,
                    &route.arm,
                    &workspace,
                    &root.join("workspaces").join(format!("{id}-validation")),
                    &directory.join("submission"),
                    v3.then_some(deadline),
                )
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    json!({"status":"harness_error","error":error.to_string(),"cases":null})
                }
            };
            if v3 && validation["status"] == "attempt_timeout" && termination == "exited" {
                termination = "wall_limit";
            }
            if validation["status"] == "attempt_cancelled" {
                termination = "cancelled";
            }
            json_new(&directory.join("submission.json"), &validation)?;
        } else if task["acceptance"] == "programs" {
            json_new(
                &directory.join("submission.json"),
                &json!({"status":"attempt_cancelled","source":null,"build":null,"cases":[],"scope":"model session cancelled; no validation processes started"}),
            )?;
        }
        if v3 && Instant::now() >= deadline && termination == "exited" {
            termination = "wall_limit";
        }
        if cancellation.is_cancelled() {
            termination = "cancelled";
        }
        json_new(
            &directory.join("outcome.json"),
            &json!({"termination":termination,"model_termination":model_termination,"exit_code":status.code(),"signal":signal,"elapsed_ns":elapsed,"attempt_total_ns":start.elapsed().as_nanos() as u64,"wall_end_ms":wall_end,"pid":pid,"pair_id":pair,"shutdown_included":true,"collector_drain_ns":collector_drain_ns}),
        )?;
        json_new(
            &directory.join("postprocess.json"),
            &json!({"snapshot_ns":snapshot_ns,"submission_validation_ns":if task["acceptance"]=="programs"{Some(validation_started.elapsed().as_nanos() as u64)}else{None},"scope":"after Codex process wait; excluded from attempt_process_ms"}),
        )?;
        seal(directory)?;
        ensure!(
            termination != "cancelled",
            "collection interrupted; partial report retained"
        );
        Ok(())
    }
}

async fn terminate(
    child: &mut tokio::process::Child,
    pid: u32,
) -> Result<std::process::ExitStatus> {
    #[cfg(not(unix))]
    let _ = pid;
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGINT);
    }
    match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
        Ok(status) => Ok(status?),
        Err(_) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            child.kill().await?;
            Ok(child.wait().await?)
        }
    }
}
