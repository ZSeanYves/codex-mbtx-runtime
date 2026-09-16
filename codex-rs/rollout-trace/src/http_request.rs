//! Correlation for actual HTTP sends inside the API retry loop.

use std::sync::Arc;

use crate::RawTraceEventContext;
use crate::RequestPurpose;
use crate::StepObservation;
use crate::TraceWriter;
use crate::agent_step::record;

/// Observes sends, including attempts cancelled before headers arrive.
#[derive(Clone, Default)]
pub struct HttpRequestTraceContext(Option<Arc<HttpScope>>);

struct HttpScope {
    writer: Arc<TraceWriter>,
    context: RawTraceEventContext,
    scope_id: String,
}

impl HttpRequestTraceContext {
    pub(crate) fn new(
        writer: Arc<TraceWriter>,
        context: RawTraceEventContext,
        inference_call_id: Option<String>,
        compaction_request_id: Option<String>,
        step_id: Option<String>,
        purpose: RequestPurpose,
    ) -> Self {
        let scope_id = uuid::Uuid::new_v4().to_string();
        record(
            &writer,
            &context,
            StepObservation::HttpScope {
                scope_id: scope_id.clone(),
                inference_call_id,
                compaction_request_id,
                step_id,
                purpose,
            },
        );
        Self(Some(Arc::new(HttpScope {
            writer,
            context,
            scope_id,
        })))
    }

    /// Called immediately before entering the transport send future.
    pub fn started(&self, attempt: u64) {
        if let Some(scope) = &self.0 {
            record(
                &scope.writer,
                &scope.context,
                StepObservation::HttpStarted {
                    scope_id: scope.scope_id.clone(),
                    request_id: format!("{}:{attempt}", scope.scope_id),
                    attempt,
                },
            );
        }
    }

    /// Headers or a send failure were observed; this is not stream completion.
    pub fn finished(&self, attempt: u64, status_code: Option<u16>, error: Option<String>) {
        if let Some(scope) = &self.0 {
            record(
                &scope.writer,
                &scope.context,
                StepObservation::HttpFinished {
                    request_id: format!("{}:{attempt}", scope.scope_id),
                    status_code,
                    error,
                },
            );
        }
    }
}
