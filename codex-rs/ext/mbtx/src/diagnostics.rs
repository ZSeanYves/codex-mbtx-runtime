//! Bounded presentation derived from compiler stderr; raw archives stay intact.
use codex_tools::ToolProcessExecutor;
use codex_tools::ToolProcessOutput;
use serde::Serialize;

const SCAN_BYTES: usize = 262_144;
const PREVIEW_BYTES: usize = 8192;

#[derive(Debug, Serialize)]
pub(crate) struct CompilerPreview {
    pub kind: &'static str,
    pub format: &'static str,
    pub text: String,
    pub scanned_bytes: usize,
    pub scan_complete: bool,
    pub display_complete: bool,
    pub errors_in_scan: usize,
    pub warnings_in_scan: usize,
}

pub(crate) fn preview(
    build: &ToolProcessOutput,
    source_path: &str,
    executor: &dyn ToolProcessExecutor,
) -> CompilerPreview {
    let mut text = build.stderr.clone();
    let mut complete = !build.stderr_truncated;
    if build.stderr_truncated
        && let Some(resource) = build
            .resources
            .iter()
            .find(|resource| resource.stream == "stderr")
    {
        let mut archived = String::new();
        while archived.len() < SCAN_BYTES {
            let remaining = SCAN_BYTES - archived.len();
            if remaining < 4 {
                break;
            }
            let Ok(page) = executor.read_output_resource(
                &resource.resource_id,
                archived.len() as u64,
                remaining.min(65536),
            ) else {
                break;
            };
            if page.next_offset <= archived.len() as u64 {
                complete = page.eof && resource.complete;
                break;
            }
            archived.push_str(&page.text);
            if page.eof {
                complete = resource.complete;
                break;
            }
        }
        if archived.len() >= text.len() {
            text = archived;
        }
    }
    if text.len() > SCAN_BYTES {
        truncate(&mut text, SCAN_BYTES);
        complete = false;
    }
    let mut preview = derive(&text, source_path, complete);
    if build.exit_code != Some(0) && preview.errors_in_scan == 0 {
        // A launcher/unknown compiler failure can follow ordinary warnings.
        // Never hide it merely because an earlier warning was recognized.
        preview.format = "raw_fallback";
        preview.text = text;
        preview.display_complete = preview.text.len() <= PREVIEW_BYTES;
        truncate(&mut preview.text, PREVIEW_BYTES);
    }
    preview
}

fn derive(text: &str, source_path: &str, scan_complete: bool) -> CompilerPreview {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut other = Vec::new();
    let mut current = String::new();
    let mut kind = "other";
    for line in text.lines() {
        let next = if line.starts_with("Error: [") {
            Some("error")
        } else if line.starts_with("Warning: [") {
            Some("warning")
        } else {
            None
        };
        if let Some(next) = next {
            if !current.is_empty() {
                match kind {
                    "error" => errors.push(std::mem::take(&mut current)),
                    "warning" => warnings.push(std::mem::take(&mut current)),
                    _ => other.push(std::mem::take(&mut current)),
                }
            }
            kind = next;
        }
        current.push_str(line);
        current.push('\n');
        if line.starts_with('─') && line.ends_with('╯') {
            match kind {
                "error" => errors.push(std::mem::take(&mut current)),
                "warning" => warnings.push(std::mem::take(&mut current)),
                _ => other.push(std::mem::take(&mut current)),
            }
            kind = "other";
        }
    }
    if !current.is_empty() {
        match kind {
            "error" => errors.push(current),
            "warning" => warnings.push(current),
            _ => other.push(current),
        }
    }
    let recognized = !errors.is_empty() || !warnings.is_empty();
    let mut rendered = if recognized {
        errors.concat() + &other.concat()
    } else {
        text.into()
    };
    if recognized {
        // Only normalize the exact compiler input, never unrelated user paths.
        rendered = rendered.replace(&format!("{source_path}:"), "source:");
        if !warnings.is_empty() {
            rendered.push_str(&format!(
                "{} compiler warning(s) omitted; raw stderr remains available.\n",
                warnings.len()
            ));
        }
    }
    let display_complete = rendered.len() <= PREVIEW_BYTES && warnings.is_empty();
    truncate(&mut rendered, PREVIEW_BYTES);
    CompilerPreview {
        kind: "derived_compiler_diagnostic",
        format: if recognized {
            "moonbit_error_first"
        } else {
            "raw_fallback"
        },
        text: rendered,
        scanned_bytes: text.len(),
        scan_complete,
        display_complete,
        errors_in_scan: errors.len(),
        warnings_in_scan: warnings.len(),
    }
}

pub(crate) fn truncate(text: &mut String, bytes: usize) {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
