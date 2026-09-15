use std::future::Future;
use std::pin::Pin;

use codex_utils_path_uri::PathUri;
use serde::Serialize;

/// A bounded direct process request. The host retains all permission authority.
pub struct ToolProcessRequest {
    pub environment_id: String,
    pub command: Vec<String>,
    pub cwd: PathUri,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub description: String,
    /// Extension-owned overrides; never taken from the model's tool arguments.
    pub env_overrides: std::collections::HashMap<String, String>,
}

/// Observed process termination, independent of a task's correctness oracle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolProcessStatus {
    Exited,
    TimedOut,
    Cancelled,
}

/// A reaped process and fully drained, bounded streams. Missing signals stay null.
#[derive(Debug, Clone, Serialize)]
pub struct ToolProcessOutput {
    pub status: ToolProcessStatus,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}

/// Invocation-scoped host capability for permission-aware process execution.
/// Implementations must honor cancellation, reap the child, and bound IO drain.
pub trait ToolProcessExecutor: Send + Sync {
    /// Check environment support and cancellation before preparing source files.
    fn check_available(&self, environment_id: &str) -> Result<(), String>;

    fn execute<'a>(
        &'a self,
        request: ToolProcessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ToolProcessOutput, String>> + Send + 'a>>;
}
