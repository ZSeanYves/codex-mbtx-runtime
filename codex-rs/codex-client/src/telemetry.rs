use codex_http_client::TransportError;
use http::StatusCode;
use std::time::Duration;

/// API specific telemetry.
pub trait RequestTelemetry: Send + Sync {
    /// Observe sends even when cancellation prevents a response callback.
    fn on_request_start(&self, _attempt: u64) {}

    fn on_request(
        &self,
        attempt: u64,
        status: Option<StatusCode>,
        error: Option<&TransportError>,
        duration: Duration,
    );
}
