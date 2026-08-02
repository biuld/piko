//! Runtime telemetry sink for agent/tool metrics.
//!
//! The hostd binary provides the OTel-backed implementation; the default is
//! a no-op so orchd stays exporter-free. Trace spans are plain `tracing`
//! spans and need no OpenTelemetry dependency here.

/// Timing and result of one completed model step.
#[derive(Debug, Clone)]
pub struct ModelStepTelemetry {
    pub model: String,
    pub provider: String,
    pub duration_ms: u64,
    pub status: &'static str,
}

/// Timing and result of one completed tool call.
#[derive(Debug, Clone)]
pub struct ToolCallTelemetry {
    pub tool: String,
    pub duration_ms: u64,
    pub status: &'static str,
    pub mode: &'static str,
}

/// Metrics sink for agent-runtime behavior. Hostd implements this with OTel
/// meters; the default implementation records nothing.
pub trait RuntimeTelemetry: Send + Sync {
    fn model_step_completed(&self, step: ModelStepTelemetry);
    fn tool_call_completed(&self, call: ToolCallTelemetry);
}

/// Default no-op sink used when hostd does not inject telemetry.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopRuntimeTelemetry;

impl RuntimeTelemetry for NoopRuntimeTelemetry {
    fn model_step_completed(&self, _step: ModelStepTelemetry) {}
    fn tool_call_completed(&self, _call: ToolCallTelemetry) {}
}
