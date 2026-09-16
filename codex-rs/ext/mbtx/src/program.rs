use codex_config::mbtx::MbtxConfig;
use codex_file_system::CopyOptions;
use codex_file_system::CreateDirectoryOptions;
use codex_file_system::GetMetadataOptions;
use codex_file_system::WriteFileOptions;
use codex_tools::FunctionCallError;
use codex_tools::ToolCall;
use codex_tools::ToolProcessOutput;
use codex_tools::ToolProcessRequest;
use codex_tools::ToolProcessStatus;
use codex_utils_path_uri::PathUri;
use serde::Serialize;
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

pub(crate) async fn execute(
    config: &MbtxConfig,
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
    };
    let outcome = async {
        if input.source.is_empty() || input.source.len() > 65536 {
            return Err("source must contain 1..65536 UTF-8 bytes".into());
        }
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
            input.source.into_bytes(),
            WriteFileOptions {
                follow_symlinks: false,
            },
            sandbox,
        )
        .await
        .map_err(|e| e.to_string())?;
        let source_path = native(&source)?;
        result.source_path = Some(source_path.clone());
        // Even frozen Moon builds take a writable dependency-cache lock. Use an
        // invocation-local copy through host FS permissions, never grant write
        // access to the user's global cache or fetch dependencies in a tool call.
        let local_cache = directory.join("dependencies").map_err(|e| e.to_string())?;
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
        fs.copy(
            &cache_source,
            &local_cache,
            CopyOptions { recursive: true },
            sandbox,
        )
        .await
        .map_err(|e| e.to_string())?;
        executor.check_available(&environment.environment_id)?;
        let target_dir = directory.join("build").map_err(|e| e.to_string())?;
        let budget = input
            .max_output_bytes
            .unwrap_or(1024)
            .clamp(1, 4096)
            .min(call.response_byte_budget(32768).saturating_sub(8192) / 6);
        result.stage = "compilation";
        let mut build = executor
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
                env_overrides: [
                    ("MOON_DEP_CACHE".into(), native(&local_cache)?),
                    (
                        "MOON_BUILD_CACHE".into(),
                        native(&directory.join("build-cache").map_err(|e| e.to_string())?)?,
                    ),
                ]
                .into(),
            })
            .await?;
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
            return Ok(());
        }
        result.stage = "execution";
        let artifact = target_dir
            .join("wasm/release/build/single/single.wasm")
            .map_err(|e| e.to_string())?;
        if !fs
            .get_metadata(
                &artifact,
                GetMetadataOptions {
                    follow_symlinks: false,
                },
                sandbox,
            )
            .await
            .map_err(|e| e.to_string())?
            .is_file
        {
            return Err("compiler did not produce the expected regular Wasm artifact".into());
        }
        let artifact_path = native(&artifact)?;
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
