//! Model-gateway telemetry sink.
//!
//! `piko-llmd` stays exporter-free: it only records metric samples through
//! this port, and the hostd binary provides the OTel-backed implementation
//! (or the default no-op). Trace spans themselves are plain `tracing` spans
//! and need no OpenTelemetry dependency.

use piko_protocol::messages::Usage;

/// Metrics sink for model-gateway behavior. Hostd implements this with OTel
/// meters; the default implementation records nothing.
pub trait GatewayTelemetry: Send + Sync {
    /// Capture one model step for the durable trajectory (F-36). Hostd
    /// persists it as an optional journal event; exporters must never block.
    fn record_model_step(&self, _record: piko_protocol::TrajectoryModelStepRecord) {}

    /// Time to first content token, in milliseconds.
    fn record_ttft(&self, model: &str, provider: &str, ttft_ms: u64);

    /// Token usage and cost for one completed model response.
    fn record_usage(&self, model: &str, provider: &str, usage: &Usage);

    /// One retry attempt with its error class.
    fn record_retry(&self, model: &str, provider: &str, error_class: &str, attempt: u32);

    /// A stream → non-streaming fallback happened.
    fn record_fallback(&self, model: &str, provider: &str);
}

/// Default no-op sink used when hostd does not inject telemetry.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopGatewayTelemetry;

impl GatewayTelemetry for NoopGatewayTelemetry {
    fn record_ttft(&self, _model: &str, _provider: &str, _ttft_ms: u64) {}
    fn record_usage(&self, _model: &str, _provider: &str, _usage: &Usage) {}
    fn record_retry(&self, _model: &str, _provider: &str, _error_class: &str, _attempt: u32) {}
    fn record_fallback(&self, _model: &str, _provider: &str) {}
}
