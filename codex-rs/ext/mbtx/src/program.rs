use crate::cache::Compiled;
use crate::cache::Prepared;
use crate::cache::SessionCache;
use codex_config::mbtx::MbtxConfig;
use codex_file_system::CopyOptions;
use codex_file_system::CreateDirectoryOptions;
use codex_file_system::GetMetadataOptions;
use codex_file_system::ReadFileOptions;
use codex_file_system::WriteFileOptions;
use codex_tools::FunctionCallError;
use codex_tools::ToolCall;
use codex_tools::ToolProcessOutput;
use codex_tools::ToolProcessRequest;
use codex_tools::ToolProcessStatus;
use codex_utils_path_uri::PathUri;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::time::Instant;
use uuid::Uuid;

use crate::tool::ProgramInput;

#[derive(Serialize)]
pub(crate) struct ProgramResult {
    pub status: &'static str,
    pub stage: &'static str,
    pub target: &'static str,
    pub source_path: Option<String>,
    pub artifact_path: Option<String>,
    pub build: Option<ToolProcessOutput>,
    pub run: Option<ToolProcessOutput>,
    pub error: Option<String>,
    pub cache: &'static str,
    pub build_reused_from: Option<String>,
    pub preparation_ms: u64,
    pub source_resource: Option<codex_tools::output_archive::OutputResource>,
    pub artifact_resource: Option<codex_tools::output_archive::OutputResource>,
}

impl ProgramResult {
    // The raw stream limit is not enough: JSON escaping can expand each byte.
    // Bound the serialized result too, marking every additional truncation.
    pub(crate) fn fit_response(&mut self, budget: usize) -> Result<(), FunctionCallError> {
        loop {
            if serde_json::to_vec(&self)
                .map_err(|error| {
                    FunctionCallError::Fatal(format!("MBTX result serialization failed: {error}"))
                })?
                .len()
                <= budget
            {
                return Ok(());
            }
            let mut shortened = false;
            for output in [&mut self.build, &mut self.run].into_iter().flatten() {
                for (text, truncated) in [
                    (&mut output.stdout, &mut output.stdout_truncated),
                    (&mut output.stderr, &mut output.stderr_truncated),
                ] {
                    if !text.is_empty() {
                        let mut length = text.len() / 2;
                        while !text.is_char_boundary(length) {
                            length -= 1;
                        }
                        text.truncate(length);
                        *truncated = true;
                        shortened = true;
                    }
                }
                // Compiler diagnostics must not evict runtime output first.
                if shortened {
                    break;
                }
            }
            if !shortened {
                return Err(FunctionCallError::RespondToModel("MBTX result metadata exceeds the host response budget; execution artifacts remain in the workspace".into()));
            }
        }
    }
}

fn native(path: &PathUri) -> Result<String, String> {
    path.to_abs_path()
        .map_err(|e| e.to_string())?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 paths are unsupported by mbtx".into())
}

#[expect(
    clippy::await_holding_invalid_type,
    reason = "preparation and compilation share one serial MoonBit build directory; release before program execution"
)]
pub(crate) async fn execute(
    config: &MbtxConfig,
    cache: &SessionCache,
    call: &ToolCall<'_>,
    input: ProgramInput,
) -> ProgramResult {
    let mut result = ProgramResult {
        status: "error",
        stage: "preparation",
        target: "wasm",
        source_path: None,
        artifact_path: None,
        build: None,
        run: None,
        error: None,
        cache: "miss",
        build_reused_from: None,
        preparation_ms: 0,
        source_resource: None,
        artifact_resource: None,
    };
    let outcome = async {
        let preparing = Instant::now();
        if input.argv.len() > 256
            || input.argv.iter().map(String::len).sum::<usize>() > 32768
            || input.argv.iter().any(|arg| arg.contains('\0'))
        {
            return Err("argv exceeds its bound or contains NUL".into());
        }
        let moon = config.moon.as_ref().ok_or("mbtx.moon is required")?;
        let moonrun = config.moonrun.as_ref().ok_or("mbtx.moonrun is required")?;
        let dependency_cache = config
            .dependency_cache
            .as_ref()
            .ok_or("mbtx.dependency_cache is required")?;
        let [environment] = call.environments.as_slice() else {
            return Err("mbtx requires exactly one selected execution environment".into());
        };
        let executor = call
            .process_executor
            .as_ref()
            .ok_or("host process execution is unavailable")?;
        executor.check_available(&environment.environment_id)?;
        let cwd = PathUri::from(environment.cwd.clone())
            .join(input.cwd.as_deref().unwrap_or("."))
            .map_err(|e| e.to_string())?;
        let fs = &environment.file_system;
        let sandbox = Some(&environment.file_system_sandbox_context);
        let cwd = fs
            .canonicalize(&cwd, sandbox)
            .await
            .map_err(|e| e.to_string())?;
        if !fs
            .get_metadata(&cwd, GetMetadataOptions::default(), sandbox)
            .await
            .map_err(|e| e.to_string())?
            .is_directory
        {
            return Err("cwd is not a directory".into());
        }
        let source_text = match (&input.source, &input.filename) {
            (Some(source), None) => source.clone(),
            (None, Some(filename)) => {
                let path = cwd.join(filename).map_err(|e| e.to_string())?;
                let metadata = fs
                    .get_metadata(&path, GetMetadataOptions::default(), sandbox)
                    .await
                    .map_err(|e| e.to_string())?;
                if !metadata.is_file {
                    return Err("filename must be a regular file".into());
                }
                if metadata.size > 65536 {
                    return Err("source exceeds 65536 UTF-8 bytes".into());
                }
                fs.read_file_text(&path, ReadFileOptions::default(), sandbox)
                    .await
                    .map_err(|e| e.to_string())?
            }
            _ => return Err("provide exactly one of source or filename".into()),
        };
        if source_text.is_empty() || source_text.len() > 65536 {
            return Err("source must contain 1..65536 UTF-8 bytes".into());
        }
        result.source_resource =
            executor.archive_program_resource("source", "utf8", source_text.as_bytes());
        let directory = cwd
            .join(&format!(".codex-mbtx/{}", Uuid::new_v4()))
            .map_err(|e| e.to_string())?;
        fs.create_directory(
            &directory,
            CreateDirectoryOptions {
                recursive: true,
                follow_symlinks: false,
            },
            sandbox,
        )
        .await
        .map_err(|e| e.to_string())?;
        executor.check_available(&environment.environment_id)?;
        let source = directory.join("program.mbtx").map_err(|e| e.to_string())?;
        fs.write_file(
            &source,
            source_text.as_bytes().to_vec(),
            WriteFileOptions {
                follow_symlinks: false,
            },
            sandbox,
        )
        .await
        .map_err(|e| e.to_string())?;
        let source_path = native(&source)?;
        result.source_path = Some(source_path.clone());
        // Taking the generation retires it if this future is cancelled during
        // preparation or compilation. Only a completed build publishes it again.
        let mut state = cache.0.lock().await;
        executor.check_available(&environment.environment_id)?;
        let initialized = state.prepared.is_some();
        let mut prepared = state.prepared.take().unwrap_or_else(|| Prepared {
            directory: directory.clone(),
            entries: Default::default(),
        });
        let local_cache = prepared
            .directory
            .join("dependencies")
            .map_err(|e| e.to_string())?;
        let cache_source = fs
            .canonicalize(&PathUri::from(dependency_cache.clone()), sandbox)
            .await
            .map_err(|e| e.to_string())?;
        if directory
            .to_abs_path()
            .map_err(|e| e.to_string())?
            .starts_with(cache_source.to_abs_path().map_err(|e| e.to_string())?)
        {
            return Err(
                "invocation scratch must not be inside the configured dependency cache".into(),
            );
        }
        if !initialized {
            fs.copy(
                &cache_source,
                &local_cache,
                CopyOptions { recursive: true },
                sandbox,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        executor.check_available(&environment.environment_id)?;
        let target_dir = prepared
            .directory
            .join("build")
            .map_err(|e| e.to_string())?;
        let budget = input
            .max_output_bytes
            .unwrap_or(65536)
            .clamp(1, 65536)
            .min(call.response_byte_budget(65536));
        let dependencies = local_cache.to_abs_path().map_err(|e| e.to_string())?;
        let compiler = std::fs::canonicalize(moon).map_err(|e| e.to_string())?;
        let mut input_roots = vec![
            compiler.clone(),
            moonrun.to_path_buf(),
            dependencies.join("v1/sources").to_path_buf(),
            dependencies.join(".moon-cache").to_path_buf(),
        ];
        if let Some(root) = compiler.parent().and_then(|bin| bin.parent()) {
            for name in ["moonc", "mooninfo", "moonfmt"] {
                let binary = root.join("bin").join(name);
                if binary.exists() {
                    input_roots.push(binary);
                }
            }
            let lib = root.join("lib");
            if lib.exists() {
                input_roots.push(lib);
            }
        }
        let inputs = crate::cache::inputs(&input_roots, &mut state.fingerprints)
            .map_err(|e| e.to_string())?;
        let key = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    &source_text,
                    &cwd,
                    config,
                    inputs,
                    "wasm",
                    "release",
                    "--frozen",
                ))
                .map_err(|e| e.to_string())?
            )
        );
        let cached = prepared.entries.get(&key).cloned();
        result.cache = if cached.is_some() {
            "hit"
        } else if initialized {
            "dependencies_reused"
        } else {
            "cold"
        };
        result.preparation_ms = preparing.elapsed().as_millis() as u64;
        result.stage = "compilation";
        let mut build = if let Some(cached) = &cached {
            result.build_reused_from = Some(cached.call_id.clone());
            cached.build.clone()
        } else {
            executor
                .execute(ToolProcessRequest {
                    environment_id: environment.environment_id.clone(),
                    command: vec![
                        moon.to_string_lossy().into_owned(),
                        "build".into(),
                        "--frozen".into(),
                        "--target".into(),
                        "wasm".into(),
                        "--release".into(),
                        "--target-dir".into(),
                        native(&target_dir)?,
                        source_path,
                    ],
                    cwd: cwd.clone(),
                    timeout_ms: input.build_timeout_ms.unwrap_or(60_000).clamp(1, 120_000),
                    max_output_bytes: budget,
                    description: "Compile submitted MBTX program under current permissions".into(),
                    phase: "build",
                    env_overrides: [
                        ("MOON_DEP_CACHE".into(), native(&local_cache)?),
                        (
                            "MOON_BUILD_CACHE".into(),
                            native(
                                &prepared
                                    .directory
                                    .join("build-cache")
                                    .map_err(|e| e.to_string())?,
                            )?,
                        ),
                    ]
                    .into(),
                })
                .await?
        };
        let original_build = build.clone();
        let build_ok = build.status == ToolProcessStatus::Exited && build.exit_code == Some(0);
        if build_ok {
            // Preserve at least half the combined stream budget for execution.
            // Failed builds retain the full budget because no run follows them.
            let mut diagnostics = budget / 2;
            for (text, truncated) in [
                (&mut build.stdout, &mut build.stdout_truncated),
                (&mut build.stderr, &mut build.stderr_truncated),
            ] {
                let mut length = diagnostics.min(text.len());
                while !text.is_char_boundary(length) {
                    length -= 1;
                }
                if length < text.len() {
                    text.truncate(length);
                    *truncated = true;
                }
                diagnostics -= length;
            }
        }
        let remaining = budget.saturating_sub(build.stdout.len() + build.stderr.len());
        result.build = Some(build);
        if !build_ok {
            if original_build.status == ToolProcessStatus::Exited {
                state.prepared = Some(prepared);
            }
            return Ok(());
        }
        result.stage = "execution";
        let compiled = if let Some(cached) = cached {
            cached
        } else {
            // Current MoonBit versions namespace standalone builds by source
            // filename. Retain compatibility with the earlier flat layout.
            let mut artifact = target_dir
                .join("program.mbtx/wasm/release/build/single/single.wasm")
                .map_err(|e| e.to_string())?;
            let options = GetMetadataOptions { follow_symlinks: false };
            let metadata = match fs.get_metadata(&artifact, options, sandbox).await {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    artifact = target_dir
                        .join("wasm/release/build/single/single.wasm")
                        .map_err(|e| e.to_string())?;
                    fs
                .get_metadata(
                    &artifact,
                    options,
                    sandbox,
                )
                .await
                }
                result => result,
            }.map_err(|e| e.to_string())?;
            if !metadata.is_file {
                return Err("compiler did not produce the expected regular Wasm artifact".into());
            }
            let bytes = fs
                .read_file(
                    &artifact,
                    ReadFileOptions {
                        follow_symlinks: false,
                    },
                    sandbox,
                )
                .await
                .map_err(|e| e.to_string())?;
            let compiled = Compiled {
                bytes,
                build: original_build,
                call_id: call.call_id.clone(),
            };
            prepared.entries.insert(key, compiled.clone());
            compiled
        };
        let execution_artifact = directory.join("program.wasm").map_err(|e| e.to_string())?;
        result.artifact_resource =
            executor.archive_program_resource("artifact", "wasm", &compiled.bytes);
        fs.write_file(
            &execution_artifact,
            compiled.bytes,
            WriteFileOptions {
                follow_symlinks: false,
            },
            sandbox,
        )
        .await
        .map_err(|e| e.to_string())?;
        state.prepared = Some(prepared);
        drop(state);
        let artifact_path = native(&execution_artifact)?;
        result.artifact_path = Some(artifact_path.clone());
        let mut command = vec![
            moonrun.to_string_lossy().into_owned(),
            "--".into(),
            artifact_path,
        ];
        command.extend(input.argv);
        let run = executor
            .execute(ToolProcessRequest {
                environment_id: environment.environment_id.clone(),
                command,
                cwd,
                timeout_ms: input.run_timeout_ms.unwrap_or(10_000).clamp(1, 60_000),
                max_output_bytes: remaining,
                description: "Execute submitted MBTX program under current permissions".into(),
                phase: "run",
                env_overrides: Default::default(),
            })
            .await?;
        if run.status == ToolProcessStatus::Exited && run.exit_code == Some(0) {
            result.status = "success";
        }
        result.run = Some(run);
        result.stage = "completed";
        Ok::<_, String>(())
    }
    .await;
    if let Err(error) = outcome {
        result.error = Some(error.chars().take(512).collect());
    }
    result
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
