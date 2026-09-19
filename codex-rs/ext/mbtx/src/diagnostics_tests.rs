use super::derive;
use pretty_assertions::assert_eq;

#[test]
fn errors_follow_warnings_in_raw_output_but_lead_the_derived_preview() {
    let raw = concat!(
        "Warning: [0002]\n /work/program.mbtx:4:1 unused value\n",
        "Error: [3002]\n /work/program.mbtx:122:35 unexpected comma\n",
        "Warning: [0041]\n /other/program.mbtx:8:2 style\n",
    );
    let preview = derive(raw, "/work/program.mbtx", true);
    assert_eq!(
        preview.text,
        concat!(
            "Error: [3002]\n source:122:35 unexpected comma\n",
            "2 compiler warning(s) omitted; raw stderr remains available.\n",
        )
    );
    assert_eq!(
        (
            preview.errors_in_scan,
            preview.warnings_in_scan,
            preview.scan_complete,
            preview.display_complete
        ),
        (1, 2, true, false)
    );
    assert_eq!(raw.lines().next(), Some("Warning: [0002]"));
}

#[test]
fn unknown_format_is_preserved_and_large_scans_are_explicitly_partial() {
    let raw = "compiler stopped: 雪\n";
    let preview = derive(raw, "/work/source", true);
    assert_eq!(
        (
            preview.text.as_str(),
            preview.format,
            preview.display_complete
        ),
        (raw, "raw_fallback", true)
    );
    let preview = derive(&"Error: [3002]\n雪".repeat(1000), "/work/source", false);
    assert!(preview.text.len() <= 8192);
    assert!(!preview.scan_complete);
    assert!(!preview.display_complete);
}

#[test]
fn normalizes_only_the_known_source_path() {
    let preview = derive(
        "Error: [3002]\n/a/source:1:2\n/b/source:3:4\n",
        "/a/source",
        true,
    );
    assert_eq!(preview.text, "Error: [3002]\nsource:1:2\n/b/source:3:4\n");
}

#[test]
fn archive_scan_finds_errors_after_the_original_preview_without_changing_receipts() {
    use codex_tools::ToolProcessExecutor;
    use codex_tools::ToolProcessOutput;
    use codex_tools::ToolProcessRequest;
    use codex_tools::ToolProcessStatus;
    use codex_tools::output_archive::OutputResource;
    use codex_tools::output_archive::ResourcePage;
    use std::future::Future;
    use std::pin::Pin;
    struct Archive(String);
    impl ToolProcessExecutor for Archive {
        fn check_available(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn execute<'a>(
            &'a self,
            _: ToolProcessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<ToolProcessOutput, String>> + Send + 'a>> {
            panic!("deriving diagnostics must never execute a process")
        }
        fn read_output_resource(
            &self,
            id: &str,
            offset: u64,
            max_bytes: usize,
        ) -> Result<ResourcePage, String> {
            let start = offset as usize;
            let mut end = (start + max_bytes).min(self.0.len());
            while !self.0.is_char_boundary(end) {
                end -= 1;
            }
            Ok(ResourcePage {
                resource_id: id.into(),
                offset,
                next_offset: end as u64,
                total_bytes: self.0.len() as u64,
                eof: end == self.0.len(),
                text: self.0[start..end].into(),
            })
        }
    }
    let raw = "Warning: [0002]\nunused value\n".repeat(3000)
        + "Error: [3002]\n/work/input:7:2 invalid match\n";
    let archive = Archive(raw.clone());
    let receipt = OutputResource {
        resource_id: "raw-stderr".into(),
        call_id: "compile-call".into(),
        phase: "build".into(),
        stream: "stderr".into(),
        bytes: raw.len() as u64,
        sha256: Some("original archive digest".into()),
        eof: true,
        complete: true,
        error: None,
        write_ns: 0,
        started_unix_ms: 0,
        ended_unix_ms: Some(1),
    };
    let mut build = ToolProcessOutput {
        status: ToolProcessStatus::Exited,
        exit_code: Some(1),
        signal: None,
        stdout: String::new(),
        stderr: raw[..64].into(),
        stdout_truncated: false,
        stderr_truncated: true,
        duration_ms: 1,
        resources: vec![receipt.clone()],
    };
    let preview = super::preview(&build, "/work/input", &archive);
    assert!(
        preview
            .text
            .starts_with("Error: [3002]\nsource:7:2 invalid match")
    );
    assert_eq!(
        (preview.scanned_bytes, preview.scan_complete),
        (raw.len(), true)
    );
    assert_eq!(build.resources, vec![receipt]);
    assert_eq!(archive.0, raw);
    build.resources[0].complete = false;
    assert!(!super::preview(&build, "/work/input", &archive).scan_complete);
    let huge = Archive(raw.repeat(4));
    assert!(!super::preview(&build, "/work/input", &huge).scan_complete);
    build.resources.clear();
    build.stderr = "Warning: [0002]\n unused value\n───╯\nlauncher failed unexpectedly\n".into();
    build.stderr_truncated = false;
    let fallback = super::preview(&build, "/work/input", &archive);
    assert_eq!(
        (fallback.format, fallback.text),
        ("raw_fallback", build.stderr)
    );
}
