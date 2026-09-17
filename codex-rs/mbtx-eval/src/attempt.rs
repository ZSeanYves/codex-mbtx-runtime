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
        fs::copy(
            bundle.join("fixture-worker"),
            workspace.join("fixture-worker"),
        )?;
        let workspace_facts = crate::workspace::prepare(
            &workspace,
            &work.join("home"),
            manifest["path"].as_str().context("recorded PATH")?,
        )
        .await?;
        json_new(&directory.join("workspace.json"), &workspace_facts)?;
        let endpoint = format!("{}/a/{id}/v1", gate.endpoint);
        let instructions = manifest["protocol"]["instructions"]
            .as_str()
            .context("instructions")?;
        // Older frozen protocols have only the common instructions. Never
        // silently give a recorded run a newer submission contract on resume.
        let instructions = match manifest["protocol"].get("arm_instructions") {
            Some(arms) if !arms.is_null() => format!(
                "{instructions}\n\n{}",
                arms[&route.arm]
                    .as_str()
                    .context("assigned arm instructions")?
            ),
            _ => instructions.to_owned(),
        };
        let toml = child_config(
            config,
            bundle,
            bundle_info,
            crate::config::AttemptContext {
                arm: &route.arm,
                endpoint: &endpoint,
                attempt_id: id,
                instructions: &instructions,
                work: &work,
            },
        )?;
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
        let start = Instant::now();
        let wall_start = now_ms();
        let mut child = command.spawn().context("launch bundled Codex")?;
        let pid = child.id().context("Codex PID")?;
        json_new(
            &directory.join("process.json"),
            &json!({"pid":pid,"pgid":pid,"wall_start_ms":wall_start,"clock_domain":format!("attempt-{id}-monotonic")}),
        )?;
        let mut termination = "exited";
        let max_wall_seconds = manifest["max_wall_seconds"]
            .as_u64()
            .context("recorded attempt wall limit")?;
        let status = tokio::select! {
            status=child.wait()=>status?,
            _=tokio::time::sleep(Duration::from_secs(max_wall_seconds))=> {termination="wall_limit"; terminate(&mut child,pid).await?},
            _=tokio::signal::ctrl_c()=> {termination="cancelled"; terminate(&mut child,pid).await?},
        };
        let elapsed = start.elapsed().as_nanos() as u64;
        route.close();
        tokio::time::timeout(Duration::from_secs(15), gate.drain())
            .await
            .context("upstream stream failed to drain; preserve open attempt")?;
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
        json_new(
            &directory.join("outcome.json"),
            &json!({"termination":termination,"exit_code":status.code(),"signal":signal,"elapsed_ns":elapsed,"wall_end_ms":now_ms(),"pid":pid,"pair_id":pair,"shutdown_included":true}),
        )?;
        json_new(
            &directory.join("snapshot.json"),
            &snapshot(&workspace, task)?,
        )?;
        if task["acceptance"] == "programs" {
            let validator = crate::submission::Validator {
                bundle,
                bundle_info,
                path: manifest["path"].as_str().context("recorded PATH")?,
            };
            let validation = match validator
                .validate(
                    task,
                    &route.arm,
                    &workspace,
                    &root.join("workspaces").join(format!("{id}-validation")),
                    &directory.join("submission"),
                )
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    json!({"status":"harness_error","error":error.to_string(),"cases":null})
                }
            };
            json_new(&directory.join("submission.json"), &validation)?;
        }
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
