//! Add raw wire observations while preserving the existing OTel observer.

use std::sync::Arc;
use std::time::Duration;

use codex_api::RequestTelemetry;
use codex_client::TransportError;
use codex_rollout_trace::HttpRequestTraceContext;
use http::StatusCode;

pub(super) struct TracedRequestTelemetry {
    pub(super) delegate: Arc<dyn RequestTelemetry>,
    pub(super) trace: HttpRequestTraceContext,
}

impl RequestTelemetry for TracedRequestTelemetry {
    fn on_request_start(&self, attempt: u64) {
        self.trace.started(attempt);
        self.delegate.on_request_start(attempt);
    }

    fn on_request(
        &self,
        attempt: u64,
        status: Option<StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    ) {
        self.trace.finished(
            attempt,
            status.map(|status| status.as_u16()),
            error.map(ToString::to_string),
        );
        self.delegate.on_request(attempt, status, error, duration);
    }
}
