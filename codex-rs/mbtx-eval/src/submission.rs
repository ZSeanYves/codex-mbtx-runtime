//! Run only the delivered source on fresh inputs. MoonBit owns the acceptance
//! oracle; this adapter records actual build, process and filesystem evidence.
use std::fs;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use tokio::process::Command;

use crate::evidence::digest;
use crate::evidence::json_new;
use crate::evidence::safe_relative;
use crate::evidence::snapshot;
use crate::evidence::write_new;

pub(crate) struct Validator<'a> {
    pub bundle: &'a Path,
    pub bundle_info: &'a Value,
    pub path: &'a str,
    pub utilities: &'a Value,
    pub cancellation: &'a crate::cancellation::Cancellation,
}

#[cfg(test)]
#[path = "submission_integration_tests.rs"]
mod tests;

impl Validator<'_> {
    pub(crate) fn command(&self, work: &Path, readable: Option<&Path>) -> Result<Command> {
        let moon_home = Path::new(
            self.bundle_info["moon_home"]
                .as_str()
                .context("MoonBit home")?,
        );
        let mut permissions = crate::workspace::permissions(work, self.bundle, moon_home)?;
        if let Some(readable) = readable {
            permissions["evaluation"]["filesystem"]
                .as_table_mut()
                .context("filesystem")?
                .insert(
                    readable.canonicalize()?.to_string_lossy().into_owned(),
                    "read".into(),
                );
        }
        let mut config = toml::Table::new();
        config.insert("default_permissions".into(), "evaluation".into());
        config.insert("permissions".into(), permissions);
        config.insert("project_doc_max_bytes".into(), 0.into());
        config.insert("project_root_markers".into(), toml::Value::Array(vec![]));
        config.insert("allow_login_shell".into(), false.into());
        let config = toml::to_string(&config)?;
        fs::create_dir_all(work.join("codex-home"))?;
        let config_path = work.join("codex-home/config.toml");
        match fs::read(&config_path) {
            Ok(existing) => ensure!(
                existing == config.as_bytes(),
                "validation sandbox configuration changed between phases"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                write_new(&config_path, config.as_bytes())?;
            }
            Err(error) => return Err(error.into()),
        }
        let mut command = Command::new(self.bundle.join("codex"));
        command.arg("sandbox");
        if work.join("worker-socket.json").exists() {
            let socket: String =
                serde_json::from_slice(&fs::read(work.join("worker-socket.json"))?)?;
            command.arg("--allow-unix-socket").arg(socket);
        }
        command
            .args(["--permission-profile", "evaluation", "-C"])
            .arg(work.join("workspace"))
            .arg("--")
            .arg(work.join("workspace/fixture-worker"))
            .arg("launch")
            .arg(work.join("tmp/entry-observed"))
            .current_dir(work.join("workspace"))
            .env_clear()
            .env("PATH", self.path)
            .env("HOME", work.join("home"))
            .env("TMPDIR", work.join("tmp"))
            .env("CODEX_HOME", work.join("codex-home"))
            .env("MOON_HOME", self.bundle.join("moon-home"))
            .env("MOON_TOOLCHAIN_ROOT", moon_home)
            .env("LANG", "C.UTF-8")
            .env("MOON_DEP_CACHE", work.join("workspace/dependencies"))
            .env("MOON_BUILD_CACHE", work.join("workspace/build-cache"));
        crate::workspace::git_environment(&mut command, &work.join("workspace"));
        if work.join("worker-socket.json").exists() {
            let socket: String =
                serde_json::from_slice(&fs::read(work.join("worker-socket.json"))?)?;
            command.env("MBTX_WORKER_SOCKET", socket);
        }
        Ok(command)
    }

    pub async fn validate(
        &self,
        task: &Value,
        arm: &str,
        source: &Path,
        work: &Path,
        evidence: &Path,
        deadline: Option<Instant>,
    ) -> Result<Value> {
        fs::create_dir(evidence)?;
        let Some(name) = crate::submission_contract::required_source(task, arm)? else {
            return Ok(json!({"status":"not_required","source":null,"build":null,"cases":[]}));
        };
        let path = source.join(&name);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(
                    json!({"status":"missing_source","expected_source":name,"source":null,"build":null,"cases":[]}),
                );
            }
            Err(error) => return Err(error.into()),
        };
        if !meta.is_file() || meta.len() > crate::submission_contract::SOURCE_LIMIT_BYTES {
            return Ok(json!({"status":"invalid_source","source":null,"build":null,"cases":[]}));
        }
        ensure!(
            path.canonicalize()?.starts_with(source.canonicalize()?),
            "submission escaped workspace"
        );
        let bytes = fs::read(&path)?;
        if std::str::from_utf8(&bytes).is_err() {
            return Ok(json!({"status":"invalid_source","source":null,"build":null,"cases":[]}));
        }
        write_new(&evidence.join(&name), &bytes)?;
        fs::create_dir(work)?;
        let started = Instant::now();
        let mut result = json!({"status":"captured","source":{"file":name,"sha256":digest(&bytes),"bytes":bytes.len()},"build":null,"cases":[],"scope":"post-submission validation; no model steps or relay calls","process_spawns":null,"phase_counts":{"compiler_starts":0,"program_starts":0},"elapsed_ns":0});
        if self.cancellation.is_cancelled() || deadline.is_some_and(|value| value <= Instant::now())
        {
            result["status"] = json!(if self.cancellation.is_cancelled() {
                "attempt_cancelled"
            } else {
                "attempt_timeout"
            });
            result["timeout_phase"] = json!("before_validation");
            return Ok(result);
        }
        let build_work = work.join("compile");
        let mut artifact = build_work
            .join("workspace/build")
            .join(&name)
            .join("wasm/release/build/single/single.wasm");
        if arm == "mbtx_program" {
            for sub in ["workspace", "home", "tmp"] {
                fs::create_dir_all(build_work.join(sub))?;
            }
            write_new(&build_work.join("workspace").join(&name), &bytes)?;
            fs::copy(
                self.bundle.join("fixture-worker"),
                build_work.join("workspace/fixture-worker"),
            )?;
            // Frozen dependency data is shared; the writable lock is private.
            for entry in walkdir::WalkDir::new(self.bundle.join("dependencies")).follow_links(false)
            {
                let entry = entry?;
                let target = build_work.join("workspace/dependencies").join(
                    entry
                        .path()
                        .strip_prefix(self.bundle.join("dependencies"))?,
                );
                if entry.file_type().is_dir() {
                    fs::create_dir_all(target)?;
                } else {
                    ensure!(entry.file_type().is_file(), "dependency symlink");
                    fs::copy(entry.path(), target)?;
                }
            }
            let mut command = self.command(&build_work, None)?;
            command
                .arg(
                    self.bundle_info["moon_path"]
                        .as_str()
                        .context("moon path")?,
                )
                .args([
                    "build",
                    "--frozen",
                    "--target",
                    "wasm",
                    "--release",
                    "--target-dir",
                    "build",
                    &name,
                ]);
            let budget = deadline.map_or(Duration::from_secs(120), |value| {
                value.saturating_duration_since(Instant::now())
            });
            let build = crate::validation_process::capture_cancellable(
                &mut command,
                &evidence.join("build"),
                budget,
                self.cancellation,
            )
            .await?;
            result["build"] = build.clone();
            result["phase_counts"]["compiler_starts"] = json!(u64::from(build["started"] == true));
            result["elapsed_ns"] = json!(started.elapsed().as_nanos() as u64);
            if build["cancelled"] == true {
                result["status"] = json!("attempt_cancelled");
                return Ok(result);
            }
            if build["timed_out"] == true {
                result["status"] = json!("attempt_timeout");
                result["timeout_phase"] = json!("compile");
                return Ok(result);
            }
            ensure!(
                build_work.join("tmp/entry-observed").is_file(),
                "validation compiler command never entered sandbox; inspect submission/build/stderr"
            );
            if build["wait_observed"] != true
                || build["drain_complete"] != true
                || build["residual_group_before_cleanup"].is_null()
            {
                result["status"] = json!("incomplete_build");
                return Ok(result);
            }
            if build["exit_code"] != 0
                || build["timed_out"] == true
                || build["residual_group_before_cleanup"] == true
            {
                result["status"] = json!("build_failed");
                return Ok(result);
            }
            if !artifact.try_exists()? {
                artifact = build_work.join("workspace/build/wasm/release/build/single/single.wasm");
            }
            ensure!(
                artifact.is_file(),
                "successful validation build produced no executable artifact"
            );
        }
        let mut cases = vec![task.clone()];
        cases.extend(
            task["withheld"]
                .as_array()
                .context("withheld cases")?
                .iter()
                .cloned(),
        );
        for (index, case) in cases.iter().enumerate() {
            if self.cancellation.is_cancelled()
                || deadline.is_some_and(|value| value <= Instant::now())
            {
                result["status"] = json!(if self.cancellation.is_cancelled() {
                    "attempt_cancelled"
                } else {
                    "attempt_timeout"
                });
                result["timeout_phase"] = json!(format!("before_case_{index}"));
                break;
            }
            let case_work = work.join(format!("case-{index}"));
            let case_evidence = evidence.join(format!("case-{index}"));
            fs::create_dir(&case_evidence)?;
            for sub in ["workspace", "home", "tmp"] {
                fs::create_dir_all(case_work.join(sub))?;
            }
            let workspace = case_work.join("workspace");
            for (path, value) in case["files"].as_object().context("case files")? {
                let path = workspace.join(safe_relative(path)?);
                fs::create_dir_all(path.parent().context("file parent")?)?;
                write_new(&path, value.as_str().context("case input text")?.as_bytes())?;
            }
            fs::copy(
                self.bundle.join("fixture-worker"),
                workspace.join("fixture-worker"),
            )?;
            write_new(&workspace.join(&name), &bytes)?;
            let remaining = deadline.map_or(Duration::from_secs(120), |value| {
                value.saturating_duration_since(Instant::now())
            });
            let case_home = case_work.join("home");
            let preparation = tokio::select! {
                facts = crate::workspace::prepare(&workspace, &case_home, self.path) => Some(facts?),
                _ = tokio::time::sleep(remaining) => {
                    result["status"] = json!("attempt_timeout");
                    None
                },
                _ = self.cancellation.cancelled() => {
                    result["status"] = json!("attempt_cancelled");
                    None
                },
            };
            let Some(preparation) = preparation else {
                result["interrupted_phase"] = json!(format!("case_{index}_preparation"));
                json_new(
                    &case_evidence.join("workspace.json"),
                    &json!({"status":result["status"],"scope":"workspace preparation interrupted; no validation program started"}),
                )?;
                break;
            };
            json_new(&case_evidence.join("workspace.json"), &preparation)?;
            let receipts = crate::worker_receipts::WorkerReceipts::start(
                &case_evidence,
                &self.bundle.join("fixture-worker"),
            )
            .await?;
            json_new(
                &case_work.join("worker-socket.json"),
                &json!(receipts.socket),
            )?;
            let policy = crate::process_policy::prepare(
                task,
                &case_work,
                &case_evidence,
                self.bundle,
                self.utilities,
            )?;
            let mut phase_facts = Vec::new();
            let mut receipt_offset = 0;
            for (phase_index, phase) in crate::submission_phases::phases(case)?.iter().enumerate() {
                let before = crate::submission_phases::preserved_hashes(&workspace, phase)?;
                crate::submission_phases::update_inputs(&workspace, phase)?;
                let entry = case_work.join("tmp/entry-observed");
                if entry.exists() {
                    fs::remove_file(entry)?;
                }
                let phase_evidence = if phase_index == 0 {
                    case_evidence.clone()
                } else {
                    let directory = case_evidence.join(format!("phase-{phase_index}"));
                    fs::create_dir(&directory)?;
                    directory
                };
                let mut command = self.command(
                    &case_work,
                    if arm == "mbtx_program" {
                        Some(&build_work)
                    } else {
                        None
                    },
                )?;
                if arm == "mbtx_program" {
                    command.arg(
                        self.bundle_info["moonrun_path"]
                            .as_str()
                            .context("moonrun path")?,
                    );
                    if let Some(policy) = &policy {
                        command
                            .arg("--policy")
                            .arg(&policy.path)
                            .env("PATH", &policy.execution_path);
                    }
                    command.arg("--").arg(&artifact);
                } else {
                    command.args(["sh", &name]);
                    if let Some(policy) = &policy {
                        command.env("PATH", format!("{}:{}", policy.execution_path, self.path));
                    }
                }
                let budget = deadline.map_or(Duration::from_secs(60), |value| {
                    value.saturating_duration_since(Instant::now())
                });
                let process = crate::validation_process::capture_cancellable(
                    &mut command,
                    &phase_evidence.join("execution"),
                    budget,
                    self.cancellation,
                )
                .await?;
                if process["timed_out"] != true && process["cancelled"] != true {
                    ensure!(
                        case_work.join("tmp/entry-observed").is_file(),
                        "validation command never entered sandbox; inspect execution stderr"
                    );
                }
                let after = crate::submission_phases::preserved_hashes(&workspace, phase)?;
                let preserved = before == after
                    && before
                        .as_object()
                        .is_some_and(|hashes| hashes.values().all(Value::is_string));
                let delivered = workspace.join(&name);
                let source_hash = if delivered
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.is_file())
                {
                    Some(digest(&fs::read(&delivered)?))
                } else {
                    None
                };
                let source_unchanged =
                    source_hash.as_deref() == result["source"]["sha256"].as_str();
                let mut snapshot = snapshot(&workspace, phase)?;
                let receipts_so_far = crate::evidence::worker_events(&case_evidence)?;
                let records = receipts_so_far.as_array().context("validation receipts")?;
                snapshot["worker_events"] = json!(&records[receipt_offset..]);
                receipt_offset = records.len();
                let facts = json!({"index":index,"phase":phase_index,
                    "input":if phase_index==0 {"fresh workspace"} else {"retained state; declared input updates only"},
                    "process":process,"snapshot":snapshot,"input_preservation":{"success":preserved,"before":before,"after":after},
                    "source_integrity":{"confidence":"observed","unchanged":source_unchanged,"sha256":source_hash},
                    "runtime_policy_sha256":policy.as_ref().map(|value| &value.sha256)});
                json_new(&phase_evidence.join("phase-facts.json"), &facts)?;
                phase_facts.push(facts);
                let starts = result["phase_counts"]["program_starts"]
                    .as_u64()
                    .unwrap_or(0);
                result["phase_counts"]["program_starts"] =
                    json!(starts + u64::from(process["started"] == true));
                if process["cancelled"] == true || process["timed_out"] == true {
                    result["status"] = json!(if process["cancelled"] == true {
                        "attempt_cancelled"
                    } else {
                        "attempt_timeout"
                    });
                    result["timeout_phase"] = json!(format!("case_{index}_phase_{phase_index}"));
                    break;
                }
                if !source_unchanged {
                    result["status"] = json!("source_changed");
                    break;
                }
                // A failed program has no valid state from which to validate a continuation.
                if process["exit_code"] != 0 || process["residual_group_before_cleanup"] == true {
                    break;
                }
            }
            receipts.finish().await?;
            let mut facts = phase_facts
                .first()
                .cloned()
                .context("missing initial validation phase")?;
            facts["phases"] = json!(phase_facts);
            json_new(&case_evidence.join("facts.json"), &facts)?;
            result["cases"].as_array_mut().context("cases")?.push(facts);
            if result["status"] != "captured" {
                break;
            }
        }

        result["elapsed_ns"] = json!(started.elapsed().as_nanos() as u64);
        Ok(result)
    }
}
