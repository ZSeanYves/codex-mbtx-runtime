//! Observations at the task loop boundary, independent of transport attempts.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use codex_protocol::models::ResponseItem;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

use crate::RawEventSeq;
use crate::RawTraceEventContext;
use crate::RawTraceEventPayload;
use crate::TraceWriter;

/// Facts preserved by the native reducer. Evaluation owns aggregation rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedStepEvent {
    pub seq: RawEventSeq,
    pub thread_id: Option<String>,
    pub codex_turn_id: Option<String>,
    pub observation: StepObservation,
}

/// Why a task sampling scope ended. Only `Accepted` records an accepted response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    Accepted,
    Failed,
    Cancelled,
    Preempted,
    Abandoned,
}

/// Purpose of a model-facing operation; auxiliary work is not a task step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPurpose {
    Task,
    Compaction,
    Auxiliary,
}

/// Additive observations in the existing append-only rollout bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum StepObservation {
    Started {
        step_id: String,
    },
    Finished {
        step_id: String,
        outcome: StepOutcome,
        response_id: Option<String>,
        reason: Option<String>,
    },
    InferenceLinked {
        step_id: Option<String>,
        inference_call_id: String,
        purpose: RequestPurpose,
    },
    ToolEmitted {
        step_id: String,
        call_id: Option<String>,
        tool_name: String,
    },
    ToolDispatched {
        step_id: String,
        tool_call_id: String,
    },
    HttpScope {
        scope_id: String,
        inference_call_id: Option<String>,
        compaction_request_id: Option<String>,
        step_id: Option<String>,
        purpose: RequestPurpose,
    },
    HttpStarted {
        scope_id: String,
        request_id: String,
        attempt: u64,
    },
    HttpFinished {
        request_id: String,
        status_code: Option<u16>,
        error: Option<String>,
    },
}

#[derive(Debug)]
struct EnabledStep {
    writer: Arc<TraceWriter>,
    context: RawTraceEventContext,
    step_id: String,
    terminal: AtomicBool,
}

/// Cloneable correlation for requests and tools, including delayed dispatch.
#[derive(Debug, Clone, Default)]
pub struct AgentStepContext(Option<Arc<EnabledStep>>);

/// Owns the sampling scope; dropping it records abandonment, never acceptance.
pub struct AgentStepGuard {
    context: AgentStepContext,
}

impl AgentStepGuard {
    pub(crate) fn start(writer: Arc<TraceWriter>, context: RawTraceEventContext) -> Self {
        let step_id = Uuid::new_v4().to_string();
        let context = AgentStepContext(Some(Arc::new(EnabledStep {
            writer,
            context,
            step_id: step_id.clone(),
            terminal: AtomicBool::new(false),
        })));
        context.record(StepObservation::Started { step_id });
        Self { context }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            context: AgentStepContext::default(),
        }
    }

    /// Correlation survives asynchronous tool execution without extending the scope.
    pub fn context(&self) -> AgentStepContext {
        self.context.clone()
    }
}

impl Drop for AgentStepGuard {
    fn drop(&mut self) {
        self.context.finish(
            StepOutcome::Abandoned,
            /*response_id*/ None,
            Some("sampling scope dropped"),
        );
    }
}

impl AgentStepContext {
    pub(crate) fn id(&self) -> Option<&str> {
        self.0.as_ref().map(|step| step.step_id.as_str())
    }

    /// Record a terminal observation exactly once, including accepted final answers.
    pub fn finish(&self, outcome: StepOutcome, response_id: Option<&str>, reason: Option<&str>) {
        let Some(step) = &self.0 else { return };
        if step.terminal.swap(true, Ordering::AcqRel) {
            return;
        }
        self.record(StepObservation::Finished {
            step_id: step.step_id.clone(),
            outcome,
            response_id: response_id.map(str::to_string),
            reason: reason.map(str::to_string),
        });
    }

    /// Observe model-emitted tools even if later dispatch or stream completion fails.
    pub fn record_output_item(&self, item: &ResponseItem) {
        let Some(step_id) = self.id() else { return };
        let (call_id, tool_name) = match item {
            ResponseItem::FunctionCall { call_id, name, .. }
            | ResponseItem::CustomToolCall { call_id, name, .. } => {
                (Some(call_id.clone()), name.clone())
            }
            ResponseItem::LocalShellCall { call_id, .. } => {
                (call_id.clone(), "local_shell".to_string())
            }
            ResponseItem::ToolSearchCall { call_id, .. } => {
                (call_id.clone(), "tool_search".to_string())
            }
            ResponseItem::WebSearchCall { id, .. } => (
                id.as_ref().map(ToString::to_string),
                "web_search".to_string(),
            ),
            ResponseItem::ImageGenerationCall { id, .. } => (
                id.as_ref().map(ToString::to_string),
                "image_generation".to_string(),
            ),
            _ => return,
        };
        self.record(StepObservation::ToolEmitted {
            step_id: step_id.to_string(),
            call_id,
            tool_name,
        });
    }

    /// Associate the canonical execution boundary with its originating round.
    pub fn record_tool_dispatch(&self, tool_call_id: &str) {
        if let Some(step_id) = self.id() {
            self.record(StepObservation::ToolDispatched {
                step_id: step_id.to_string(),
                tool_call_id: tool_call_id.to_string(),
            });
        }
    }

    fn record(&self, observation: StepObservation) {
        if let Some(step) = &self.0 {
            record(&step.writer, &step.context, observation);
        }
    }
}

pub(crate) fn record(
    writer: &TraceWriter,
    context: &RawTraceEventContext,
    observation: StepObservation,
) {
    if let Err(error) = writer.append_with_context(
        context.clone(),
        RawTraceEventPayload::StepObserved { observation },
    ) {
        tracing::warn!("failed to record step observation: {error:#}");
    }
}

#[cfg(test)]
#[path = "agent_step_tests.rs"]
mod tests;
