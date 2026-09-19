//! Shared, read-only access to host-registered references and output resources.
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
use codex_tools::ToolSpec;
use codex_tools::output_archive::ResourcePage;
use codex_tools::output_archive::read_text_page;
use serde::Deserialize;

pub(crate) struct ResourceTool(pub MbtxConfig);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    resource_id: String,
    #[serde(default)]
    offset: u64,
    max_bytes: Option<usize>,
}

impl<'call> ToolExecutor<ToolCall<'call>> for ResourceTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("read_resource")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "read_resource".into(),
            description: "Read a page of a registered read-only resource. Start with reference:moonbit for the API index; reference:syntax, reference:files, reference:collections and reference:processes cover exact installed APIs. reference:tools describes direct utilities, reference:examples is the verified executable API example, reference:shell is for the Shell arm. Output IDs come from execution tools. offset and next_offset are UTF-8 byte offsets; follow next_offset until eof. No arbitrary paths, other sessions, compilation or execution.".into(),
            strict: false,
            parameters: JsonSchema::object([
                ("resource_id".into(), JsonSchema::string(None)),
                ("offset".into(), JsonSchema::integer(None)),
                ("max_bytes".into(), JsonSchema::integer(Some("Requested page size, 4..65536; bounded by the common context allowance.".into()))),
            ].into(), Some(vec!["resource_id".into()]), Some(false.into())),
            output_schema: None, defer_loading: None,
        })
    }

    fn handle<'a>(&'a self, call: ToolCall<'call>) -> ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            let input: Input = serde_json::from_str(call.function_arguments()?)
                .map_err(|e| FunctionCallError::RespondToModel(e.to_string()))?;
            let capacity = call.response_byte_budget(65536);
            let requested = input.max_bytes.unwrap_or(8192);
            if !(4..=65536).contains(&requested) || capacity < 4 {
                return Err(FunctionCallError::RespondToModel(
                    "Page size must be 4..65536 and fit the current context allowance".into(),
                ));
            }
            let max_bytes = requested.min(capacity);
            let result = if let Some(name) = input.resource_id.strip_prefix("reference:") {
                let file = match name {
                    "moonbit" => "moonbit.md",
                    "shell" => "shell.md",
                    "tools" => "tools.md",
                    "examples" => "examples.mbtx",
                    "syntax" => "syntax.md",
                    "files" => "files.md",
                    "collections" => "collections.md",
                    "processes" => "processes.md",
                    "tool-example" => "tool-example.mbtx",
                    "process-examples" => "process-examples.mbtx",
                    _ => {
                        return Err(FunctionCallError::RespondToModel(
                            "Unknown registered reference".into(),
                        ));
                    }
                };
                let root = self.0.reference_directory.as_ref().ok_or_else(|| {
                    FunctionCallError::RespondToModel("No reference bundle configured".into())
                })?;
                read_text_page(
                    &root.join(file),
                    &input.resource_id,
                    input.offset,
                    max_bytes,
                )
                .map_err(|error| error.to_string())
            } else {
                let executor = call.process_executor.as_ref().ok_or_else(|| {
                    FunctionCallError::RespondToModel("Host output resources unavailable".into())
                })?;
                executor.read_output_resource(&input.resource_id, input.offset, max_bytes)
            }
            .map_err(|e| {
                FunctionCallError::RespondToModel(format!("Cannot read registered resource: {e}"))
            })?;
            let result = fit_page(result, capacity)?;
            let value = serde_json::to_value(result)
                .map_err(|e| FunctionCallError::Fatal(e.to_string()))?;
            Ok(Box::new(JsonToolOutput::with_success(value, Some(true))) as Box<dyn ToolOutput>)
        })
    }
}

// Read once, then fit the actual serialized response. UTF-8 boundaries and JSON
// escapes both matter; a worst-case expansion factor wastes ordinary pages.
fn fit_page(mut page: ResourcePage, budget: usize) -> Result<ResourcePage, FunctionCallError> {
    let text = std::mem::take(&mut page.text);
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([text.len()])
        .collect();
    let mut low = 0;
    let mut high = boundaries.len();
    while low < high {
        let candidate = (low + high) / 2;
        let end = boundaries[candidate];
        page.text = text[..end].into();
        page.next_offset = page.offset + end as u64;
        page.eof = page.next_offset == page.total_bytes;
        let size = serde_json::to_vec(&page)
            .map_err(|error| FunctionCallError::Fatal(error.to_string()))?
            .len();
        if size <= budget {
            low = candidate + 1;
        } else {
            high = candidate;
        }
    }
    if low == 0 || (low == 1 && !text.is_empty()) {
        return Err(FunctionCallError::RespondToModel(
            "Resource metadata and next character exceed the current response budget".into(),
        ));
    }
    let end = boundaries[low - 1];
    page.text = text[..end].into();
    page.next_offset = page.offset + end as u64;
    page.eof = page.next_offset == page.total_bytes;
    Ok(page)
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
