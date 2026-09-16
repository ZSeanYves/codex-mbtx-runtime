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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProgramInput {
    pub source: String,
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
        ToolSpec::Function(ResponsesApiTool {
            name: "mbtx".into(),
            description: format!(
                "Compile and execute one complete MoonBit .mbtx program using wasm and moonrun. Import paths are quoted strings, with versions before package suffixes. Dependencies must already be installed; no downloads occur during a call. The pilot preinstalls moonbitlang/async@0.21.3, including fs, shell and stdio. Use async fn main for its IO APIs; synchronous throwing code uses fn main raise. JSON: @json.parse(text) and value.stringify(); file text: @fs.read_file(path).text(); listing: @fs.readdir(path). Sources are preserved verbatim. File and process operations use Codex permissions. argv strings are literal; argv[0] is the runtime program name. Inherited environment and stdin EOF apply. Each call has a fresh process and heap; owned background processes terminate at completion. Returns separate build/run outcomes and bounded stdout/stderr with truncation flags. Build/run defaults are 60/10 seconds (max 120/60). Combined streams: max 4096 bytes, default 1024; successful build diagnostics use at most half, reserving runtime output. Use files for larger results. Generic file/JSON/child example requiring the above dependency:\n```moonbit\n{}```",
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
                Some(vec!["source".into()]),
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
            let mut result = execute(&self.config, &call, input).await;
            result.fit_response(call.response_byte_budget(8192))?;
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
