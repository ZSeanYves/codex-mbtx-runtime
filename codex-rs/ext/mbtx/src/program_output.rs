//! Model-visible result fitting; full streams are owned by the host archive.
use codex_tools::FunctionCallError;
use codex_tools::ToolProcessOutput;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct ProgramResult {
    pub status: &'static str,
    pub stage: &'static str,
    pub target: &'static str,
    pub source_path: Option<String>,
    pub artifact_path: Option<String>,
    pub build: Option<ToolProcessOutput>,
    pub compiler_preview: Option<crate::diagnostics::CompilerPreview>,
    pub run: Option<ToolProcessOutput>,
    pub error: Option<String>,
    pub cache: &'static str,
    pub build_reused_from: Option<String>,
    pub preparation_ms: u64,
    pub source_resource: Option<codex_tools::output_archive::OutputResource>,
    pub artifact_resource: Option<codex_tools::output_archive::OutputResource>,
    pub policy_sha256: Option<String>,
    /// A raw stderr diagnostic, not an authenticated subprocess audit event.
    pub process_denial_diagnostic: Option<&'static str>,
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
            for output in self.build.iter_mut() {
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
            if !shortened
                && let Some(preview) = &mut self.compiler_preview
                && !preview.text.is_empty()
            {
                let length = preview.text.len() / 2;
                crate::diagnostics::truncate(&mut preview.text, length);
                preview.display_complete = false;
                shortened = true;
            }
            if !shortened && let Some(output) = &mut self.run {
                for (text, truncated) in [
                    (&mut output.stdout, &mut output.stdout_truncated),
                    (&mut output.stderr, &mut output.stderr_truncated),
                ] {
                    if !text.is_empty() {
                        crate::diagnostics::truncate(text, text.len() / 2);
                        *truncated = true;
                        shortened = true;
                    }
                }
            }
            if !shortened {
                return Err(FunctionCallError::RespondToModel("MBTX result metadata exceeds the host response budget; execution artifacts remain in the workspace".into()));
            }
        }
    }
}

#[cfg(test)]
#[path = "program_tests.rs"]
mod tests;
