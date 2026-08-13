//! OTel-backed telemetry sinks for gateway/agent/turn metrics.
//!
//! Hostd owns the OTel SDK (initialized in `logging`). This module turns the
//! global meter into the `GatewayTelemetry` / `RuntimeTelemetry` ports
//! consumed by llmd/orchd, and records turn metrics directly. When
//! observability is disabled every instrument is `None` and all recording is
//! a no-op.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::trace::TraceContextExt as _;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use piko_llmd::telemetry::GatewayTelemetry;
use piko_orchd_api::telemetry::{ModelStepTelemetry, RuntimeTelemetry, ToolCallTelemetry};
use piko_protocol::messages::Usage;

static TELEMETRY: OnceLock<Arc<Telemetry>> = OnceLock::new();
static NOOP: OnceLock<Arc<Telemetry>> = OnceLock::new();

/// Initialize the process-wide telemetry handle. Called from `logging::init`
/// after the global meter provider is installed.
/// Initialize the process-wide telemetry handle. Called from `logging::init`
/// after the global meter provider is installed.
pub fn init(enabled: bool, capture_content: bool) {
    let telemetry = if enabled {
        Telemetry::from_meter(opentelemetry::global::meter("piko-hostd"), capture_content)
    } else {
        Telemetry::disabled()
    };
    let _ = TELEMETRY.set(Arc::new(telemetry));
}

/// The process-wide telemetry sink (no-op when observability is disabled).
pub fn handle() -> Arc<Telemetry> {
    if let Some(telemetry) = TELEMETRY.get() {
        Arc::clone(telemetry)
    } else {
        Arc::clone(NOOP.get_or_init(|| Arc::new(Telemetry::disabled())))
    }
}

pub struct Telemetry {
    prompt_inputs: Mutex<HashMap<(String, String), PromptInputBuffer>>,
    capture_content: bool,
    model_step_duration_ms: Option<Histogram<f64>>,
    model_step_calls: Option<Counter<u64>>,
    model_ttft_ms: Option<Histogram<f64>>,
    model_tokens: Option<Counter<u64>>,
    model_cost: Option<Counter<f64>>,
    model_retries: Option<Counter<u64>>,
    model_fallbacks: Option<Counter<u64>>,
    tool_duration_ms: Option<Histogram<f64>>,
    tool_calls: Option<Counter<u64>>,
    turn_duration_ms: Option<Histogram<f64>>,
    turn_calls: Option<Counter<u64>>,
    turn_tokens: Option<Counter<u64>>,
    turn_cost: Option<Counter<f64>>,
}

struct PromptInputBuffer {
    run_id: String,
    records: Vec<piko_protocol::ModelInputDebugSnapshot>,
}

impl Telemetry {
    fn disabled() -> Self {
        Self {
            prompt_inputs: Mutex::new(HashMap::new()),
            capture_content: false,
            model_step_duration_ms: None,
            model_step_calls: None,
            model_ttft_ms: None,
            model_tokens: None,
            model_cost: None,
            model_retries: None,
            model_fallbacks: None,
            tool_duration_ms: None,
            tool_calls: None,
            turn_duration_ms: None,
            turn_calls: None,
            turn_tokens: None,
            turn_cost: None,
        }
    }

    fn from_meter(meter: Meter, capture_content: bool) -> Self {
        Self {
            prompt_inputs: Mutex::new(HashMap::new()),
            capture_content,
            model_step_duration_ms: Some(
                meter.f64_histogram("piko.model.step.duration_ms").build(),
            ),
            model_step_calls: Some(meter.u64_counter("piko.model.step.calls").build()),
            model_ttft_ms: Some(meter.f64_histogram("piko.model.ttft_ms").build()),
            model_tokens: Some(meter.u64_counter("piko.model.tokens").build()),
            model_cost: Some(meter.f64_counter("piko.model.cost").build()),
            model_retries: Some(meter.u64_counter("piko.model.retries").build()),
            model_fallbacks: Some(meter.u64_counter("piko.model.streaming_fallbacks").build()),
            tool_duration_ms: Some(meter.f64_histogram("piko.tool.duration_ms").build()),
            tool_calls: Some(meter.u64_counter("piko.tool.calls").build()),
            turn_duration_ms: Some(meter.f64_histogram("piko.turn.duration_ms").build()),
            turn_calls: Some(meter.u64_counter("piko.turn.calls").build()),
            turn_tokens: Some(meter.u64_counter("piko.turn.tokens").build()),
            turn_cost: Some(meter.f64_counter("piko.turn.cost").build()),
        }
    }

    /// Record one completed turn (hostd side).
    pub fn record_turn(&self, duration_ms: u64, status: &str, source: &str) {
        if let Some(histogram) = &self.turn_duration_ms {
            histogram.record(
                duration_ms as f64,
                &[
                    KeyValue::new("status", status.to_string()),
                    KeyValue::new("source", source.to_string()),
                ],
            );
        }
        if let Some(counter) = &self.turn_calls {
            counter.add(
                1,
                &[
                    KeyValue::new("status", status.to_string()),
                    KeyValue::new("source", source.to_string()),
                ],
            );
        }
    }

    /// Project turn usage from the hostd ledger into turn-level OTel counters.
    pub fn record_turn_usage(&self, usage: &Usage, status: &str) {
        if let Some(counter) = &self.turn_tokens {
            for (token_type, count) in [
                ("input", usage.input),
                ("output", usage.output),
                ("cache_read", usage.cache_read),
                ("cache_write", usage.cache_write),
            ] {
                if count == 0 {
                    continue;
                }
                counter.add(
                    count,
                    &[
                        KeyValue::new("token_type", token_type),
                        KeyValue::new("status", status.to_string()),
                    ],
                );
            }
        }
        if let Some(counter) = &self.turn_cost {
            for cost in &usage.cost.entries {
                counter.add(
                    cost.total,
                    &[
                        KeyValue::new("status", status.to_string()),
                        KeyValue::new("currency", cost.currency.clone()),
                        KeyValue::new("basis", cost.basis.as_str()),
                    ],
                );
            }
        }
    }

    fn model_provider(model: &str, provider: &str) -> Vec<KeyValue> {
        vec![
            KeyValue::new("model", model.to_string()),
            KeyValue::new("provider", provider.to_string()),
        ]
    }

    pub fn begin_prompt_run(&self, session_id: &str, agent_instance_id: &str, run_id: &str) {
        self.prompt_inputs.lock().unwrap().insert(
            (session_id.to_string(), agent_instance_id.to_string()),
            PromptInputBuffer {
                run_id: run_id.to_string(),
                records: Vec::new(),
            },
        );
    }

    pub fn model_inputs(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        run_id: &str,
    ) -> Vec<piko_protocol::ModelInputDebugSnapshot> {
        self.prompt_inputs
            .lock()
            .unwrap()
            .get(&(session_id.to_string(), agent_instance_id.to_string()))
            .filter(|buffer| buffer.run_id == run_id)
            .map(|buffer| buffer.records.clone())
            .unwrap_or_default()
    }
}

impl GatewayTelemetry for Telemetry {
    fn capture_content(&self) -> bool {
        self.capture_content
    }

    fn record_genai_content(&self, content: &piko_llmd::telemetry::GenAiContentAttributes) {
        let context = tracing::Span::current().context();
        let span = context.span();
        if let Some(value) = content.system_instructions.as_ref() {
            span.set_attribute(KeyValue::new("gen_ai.system_instructions", value.clone()));
        }
        if let Some(value) = content.input_messages.as_ref() {
            span.set_attribute(KeyValue::new("gen_ai.input.messages", value.clone()));
        }
        if let Some(value) = content.tool_definitions.as_ref() {
            span.set_attribute(KeyValue::new("gen_ai.tool.definitions", value.clone()));
        }
        if content.dropped {
            span.set_attribute(KeyValue::new("piko.gen_ai.content_dropped", true));
        }
    }

    fn record_model_input(&self, input: piko_protocol::ModelInputDebugSnapshot) {
        const MAX_MODEL_INPUTS: usize = 32;
        let key = (input.session_id.clone(), input.agent_instance_id.clone());
        let mut inputs = self.prompt_inputs.lock().unwrap();
        let Some(buffer) = inputs.get_mut(&key) else {
            return;
        };
        // A prior run may finish a model step after a newer assembly has
        // replaced the latest snapshot. Never attach that stale input to the
        // newer run merely because session and agent identities match.
        if buffer.run_id != input.run_id {
            return;
        }
        buffer.records.push(input);
        if buffer.records.len() > MAX_MODEL_INPUTS {
            buffer.records.remove(0);
        }
    }

    fn record_ttft(&self, model: &str, provider: &str, ttft_ms: u64) {
        if let Some(histogram) = &self.model_ttft_ms {
            histogram.record(ttft_ms as f64, &Self::model_provider(model, provider));
        }
    }

    fn record_usage(&self, model: &str, provider: &str, usage: &Usage) {
        if let Some(counter) = &self.model_tokens {
            let base = Self::model_provider(model, provider);
            for (token_type, count) in [
                ("input", usage.input),
                ("output", usage.output),
                ("cache_read", usage.cache_read),
                ("cache_write", usage.cache_write),
            ] {
                let mut attributes = base.clone();
                attributes.push(KeyValue::new("token_type", token_type));
                counter.add(count, &attributes);
            }
        }
        if let Some(counter) = &self.model_cost {
            for cost in &usage.cost.entries {
                let mut attributes = Self::model_provider(model, provider);
                attributes.push(KeyValue::new("currency", cost.currency.clone()));
                attributes.push(KeyValue::new("basis", cost.basis.as_str()));
                counter.add(cost.total, &attributes);
            }
        }
    }

    fn record_retry(&self, model: &str, provider: &str, error_class: &str, _attempt: u32) {
        if let Some(counter) = &self.model_retries {
            let mut attributes = Self::model_provider(model, provider);
            attributes.push(KeyValue::new("error_class", error_class.to_string()));
            counter.add(1, &attributes);
        }
    }

    fn record_fallback(&self, model: &str, provider: &str) {
        if let Some(counter) = &self.model_fallbacks {
            counter.add(1, &Self::model_provider(model, provider));
        }
    }
}

impl RuntimeTelemetry for Telemetry {
    fn model_step_completed(&self, step: ModelStepTelemetry) {
        if let Some(histogram) = &self.model_step_duration_ms {
            histogram.record(
                step.duration_ms as f64,
                &Self::model_provider(&step.model, &step.provider),
            );
        }
        if let Some(counter) = &self.model_step_calls {
            let mut attributes = Self::model_provider(&step.model, &step.provider);
            attributes.push(KeyValue::new("status", step.status.to_string()));
            counter.add(1, &attributes);
        }
    }

    fn tool_call_completed(&self, call: ToolCallTelemetry) {
        if let Some(histogram) = &self.tool_duration_ms {
            histogram.record(
                call.duration_ms as f64,
                &[
                    KeyValue::new("tool", call.tool.clone()),
                    KeyValue::new("status", call.status.to_string()),
                    KeyValue::new("mode", call.mode.to_string()),
                ],
            );
        }
        if let Some(counter) = &self.tool_calls {
            counter.add(
                1,
                &[
                    KeyValue::new("tool", call.tool),
                    KeyValue::new("status", call.status.to_string()),
                    KeyValue::new("mode", call.mode.to_string()),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use piko_llmd::telemetry::GatewayTelemetry;

    use super::Telemetry;

    fn input(run_id: &str, step_id: &str) -> piko_protocol::ModelInputDebugSnapshot {
        piko_protocol::ModelInputDebugSnapshot {
            session_id: "session-1".into(),
            agent_instance_id: "agent-1".into(),
            run_id: run_id.into(),
            step_id: step_id.into(),
            provider: "test".into(),
            model: "test-model".into(),
            request: serde_json::json!({"step": step_id}),
            options: serde_json::json!({}),
        }
    }

    #[test]
    fn prompt_inputs_are_bound_to_the_latest_assembly_run() {
        let telemetry = Telemetry::disabled();
        telemetry.begin_prompt_run("session-1", "agent-1", "run-old");
        telemetry.record_model_input(input("run-old", "step-1"));

        telemetry.begin_prompt_run("session-1", "agent-1", "run-new");
        telemetry.record_model_input(input("run-old", "late-step"));
        telemetry.record_model_input(input("run-new", "step-1"));

        assert!(
            telemetry
                .model_inputs("session-1", "agent-1", "run-old")
                .is_empty()
        );
        let current = telemetry.model_inputs("session-1", "agent-1", "run-new");
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].step_id, "step-1");
    }
}
