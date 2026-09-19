use std::time::Duration;

use codex_config::mbtx::MbtxConfig;
use codex_tools::FunctionCallError;
use codex_tools::JsonSchema;
use codex_tools::JsonToolOutput;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolCall;
use codex_tools::ToolExecutor;
use codex_tools::ToolExecutorFuture;
use codex_tools::ToolName;
use codex_tools::ToolOutput;
use codex_tools::ToolPayload;
use codex_tools::ToolSpec;
use serde::Deserialize;

use crate::program::execute;

pub(crate) struct MbtxTool {
    pub(crate) config: MbtxConfig,
    pub(crate) cache: std::sync::Arc<crate::cache::SessionCache>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgramInput {
    pub source: Option<String>,
    pub filename: Option<String>,
    #[serde(default)]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub build_timeout_ms: Option<u64>,
    pub run_timeout_ms: Option<u64>,
    pub max_output_bytes: Option<usize>,
}

impl<'call> ToolExecutor<ToolCall<'call>> for MbtxTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("mbtx")
    }

    fn cancellation_grace_period(&self) -> Duration {
        Duration::from_secs(8)
    }

    fn spec(&self) -> ToolSpec {
        let timeout_description = if self.config.attempt_deadline_unix_ms.is_some() {
            "Compilation and execution share the host attempt deadline; per-call build_timeout_ms and run_timeout_ms are ignored."
        } else {
            "Build/run defaults are 60/10 seconds (max 120/60)."
        };
        ToolSpec::Function(ResponsesApiTool {
            name: "mbtx".into(),
            description: format!(
                "Compile and execute a complete MoonBit .mbtx program with wasm and moonrun. Provide exactly one of source (inline UTF-8) or filename (an existing file relative to cwd). Sources are snapshotted verbatim. Dependencies are preinstalled; no downloads occur during a call. moonbitlang/async@0.21.3 supplies fs, shell and stdio. Its @shell.Cmd(program, args) directly starts a program with literal arguments, without interpreting Shell syntax. Only call programs and argument forms allowed by the current task; policy and execution PATH are selected by the host. Use async fn main for IO; synchronous throwing code uses fn main raise. Import packages explicitly. Before guessing APIs, read the relevant reference:syntax, reference:files, reference:collections or reference:processes topic; reference:moonbit indexes them. When allowed, use jq for substantial JSON transformation and rg for search, with MoonBit control flow and checked exit status. compiler_preview is derived and error-first; original build/run resource IDs retain raw diagnostics. JSON construction uses typed Json literals or to_json; String is already owned, while split/trim return views. File/process operations use Codex permissions. argv is literal; argv[0] is the runtime program name. Each execution starts a fresh process and heap; background children terminate at completion. Session dependencies and exact compiled programs may be reused; cache and build_reused_from identify reuse. {timeout_description} Previews share the host context allowance; full output resources are separate when configured. Verified direct jq example (only when jq is allowed; exact source at reference:tool-example):\n```moonbit\n{}```",
                include_str!("example.mbtx")
            ),
            strict: false,
            parameters: JsonSchema::object(
                [
                    (
                        "source".into(),
                        JsonSchema::string(Some(
                            "Complete UTF-8 .mbtx program, at most 65536 bytes.".into(),
                        )),
                    ),
                    ("filename".into(), JsonSchema::string(Some("Existing UTF-8 .mbtx file; use exactly one of source or filename. The executed bytes are snapshotted.".into()))),
                    (
                        "argv".into(),
                        JsonSchema::array(JsonSchema::string(None), /*description*/ None),
                    ),
                    (
                        "cwd".into(),
                        JsonSchema::string(Some(
                            "Working directory, relative to the selected environment or absolute."
                                .into(),
                        )),
                    ),
                    ("build_timeout_ms".into(), JsonSchema::integer(None)),
                    ("run_timeout_ms".into(), JsonSchema::integer(None)),
                    ("max_output_bytes".into(), JsonSchema::integer(None)),
                ]
                .into(),
                Some(vec![]),
                Some(false.into()),
            ),
            output_schema: None,
            defer_loading: None,
        })
    }

    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            let ToolPayload::Function { arguments } = &call.payload else {
                return Err(FunctionCallError::RespondToModel(
                    "mbtx expects function arguments".into(),
                ));
            };
            let input: ProgramInput = serde_json::from_str(arguments).map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "invalid mbtx arguments: {}",
                    err.to_string().chars().take(512).collect::<String>()
                ))
            })?;
            let mut result = execute(&self.config, &self.cache, &call, input).await;
            result.fit_response(call.response_byte_budget(65536))?;
            let success = result.status == "success";
            Ok(Box::new(JsonToolOutput::with_success(
                serde_json::to_value(result).map_err(|error| {
                    FunctionCallError::Fatal(format!("MBTX result serialization failed: {error}"))
                })?,
                Some(success),
            )) as Box<dyn ToolOutput>)
        })
    }
}
