//! Observes split output without changing the unified-exec session contract.
use codex_tools::output_archive::{OutputArchive, OutputResource};
use codex_utils_pty::OutputObserver;
use crate::tools::sandboxing::ToolCtx;
use super::UnifiedExecError;

#[derive(Default, Clone)]
pub(super) struct Archives(pub Vec<OutputArchive>);

impl Archives {
    pub fn create(context: Option<&ToolCtx>, tty: bool) -> Result<Self, UnifiedExecError> {
        let Some(context) = context else { return Ok(Self::default()); };
        let Some(root) = &context.step_context.turn.config.mbtx.output_directory else { return Ok(Self::default()); };
        let streams = [if tty { "pty" } else { "stdout" }, "stderr"];
        streams.into_iter().map(|stream| OutputArchive::create(&root.join(context.session.thread_id.to_string()), &context.call_id, "shell", stream))
            .collect::<std::io::Result<Vec<_>>>().map(Self)
            .map_err(|e| UnifiedExecError::create_process(format!("cannot prepare output evidence: {e}")))
    }

    pub fn receipts(&self) -> Vec<OutputResource> { self.0.iter().map(OutputArchive::receipt).collect() }

    pub fn observe(&self, stream: codex_exec_server::ExecOutputStream, bytes: &[u8]) {
        let index = match stream { codex_exec_server::ExecOutputStream::Stdout | codex_exec_server::ExecOutputStream::Pty => 0, codex_exec_server::ExecOutputStream::Stderr => 1 };
        if let Some(archive) = self.0.get(index) { archive.append(bytes); }
    }

    pub fn finish(&self) { for archive in &self.0 { archive.finish(); } }

    pub fn gap(&self) { for archive in &self.0 { archive.record_error("exec-server output sequence gap"); } }
}

impl Drop for Archives {
    fn drop(&mut self) { for archive in &self.0 { archive.interrupted(); } }
}

impl OutputObserver for Archives {
    fn stdout(&mut self, bytes: &[u8]) { if let Some(a) = self.0.first() { a.append(bytes); } }
    fn stderr(&mut self, bytes: &[u8]) { if let Some(a) = self.0.get(1) { a.append(bytes); } }
    fn stdout_eof(&mut self) { if let Some(a) = self.0.first() { a.finish(); } }
    fn stderr_eof(&mut self) { if let Some(a) = self.0.get(1) { a.finish(); } }
}
