//! Run only the delivered source on fresh inputs. MoonBit owns the acceptance
//! oracle; this adapter records actual build, process and filesystem evidence.
use std::fs;
use std::path::Path;
use std::time::Duration;

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
}

impl Validator<'_> {
    fn command(&self, work: &Path, readable: Option<&Path>) -> Result<Command> {
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
        fs::create_dir(work.join("codex-home"))?;
        write_new(&work.join("codex-home/config.toml"), config.as_bytes())?;
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
    ) -> Result<Value> {
        fs::create_dir(evidence)?;
        let name = if arm == "mbtx_program" {
            "solution.mbtx"
        } else {
            "solution.sh"
        };
        let path = source.join(name);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(
                    json!({"status":"missing_source","expected_source":name,"source":null,"build":null,"cases":[]}),
                );
            }
            Err(error) => return Err(error.into()),
        };
        if !meta.is_file() || meta.len() > 131072 {
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
        write_new(&evidence.join(name), &bytes)?;
        fs::create_dir(work)?;
        let mut result = json!({"status":"captured","source":{"file":name,"sha256":digest(&bytes),"bytes":bytes.len()},"build":null,"cases":[],"scope":"post-submission validation; no model steps or relay calls","process_spawns":null});
        let build_work = work.join("compile");
        let artifact = build_work.join("workspace/build/wasm/release/build/single/single.wasm");
        if arm == "mbtx_program" {
            for sub in ["workspace", "home", "tmp"] {
                fs::create_dir_all(build_work.join(sub))?;
            }
            write_new(&build_work.join("workspace").join(name), &bytes)?;
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
                    name,
                ]);
            let build = crate::validation_process::capture(
                &mut command,
                &evidence.join("build"),
                Duration::from_secs(120),
            )
            .await?;
            result["build"] = build.clone();
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
            write_new(&workspace.join(name), &bytes)?;
            json_new(
                &case_evidence.join("workspace.json"),
                &crate::workspace::prepare(&workspace, &case_work.join("home"), self.path).await?,
            )?;
            let receipts = crate::worker_receipts::WorkerReceipts::start(
                &case_evidence,
                &self.bundle.join("fixture-worker"),
            )
            .await?;
            json_new(
                &case_work.join("worker-socket.json"),
                &json!(receipts.socket),
            )?;
            let mut command = self.command(
                &case_work,
                if arm == "mbtx_program" {
                    Some(&build_work)
                } else {
                    None
                },
            )?;
            if arm == "mbtx_program" {
                command
                    .arg(
                        self.bundle_info["moonrun_path"]
                            .as_str()
                            .context("moonrun path")?,
                    )
                    .arg("--")
                    .arg(&artifact);
            } else {
                command.args(["sh", name]);
            }
            let process = crate::validation_process::capture(
                &mut command,
                &case_evidence.join("execution"),
                Duration::from_secs(60),
            )
            .await?;
            ensure!(
                case_work.join("tmp/entry-observed").is_file(),
                "validation command never entered sandbox; inspect case execution stderr"
            );
            receipts.finish().await?;
            let mut snapshot = snapshot(&workspace, case)?;
            snapshot["worker_events"] = crate::evidence::worker_events(&case_evidence)?;
            let facts = json!({"index":index,"input":"fresh workspace","process":process,"snapshot":snapshot});
            json_new(&case_evidence.join("facts.json"), &facts)?;
            result["cases"].as_array_mut().context("cases")?.push(facts);
        }
        Ok(result)
    }
}
