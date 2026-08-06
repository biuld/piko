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

use piko_llmd::telemetry::GatewayTelemetry;
use piko_orchd_api::telemetry::{ModelStepTelemetry, RuntimeTelemetry, ToolCallTelemetry};
use piko_protocol::messages::Usage;

static TELEMETRY: OnceLock<Arc<Telemetry>> = OnceLock::new();
static NOOP: OnceLock<Arc<Telemetry>> = OnceLock::new();

/// Initialize the process-wide telemetry handle. Called from `logging::init`
/// after the global meter provider is installed.
/// Initialize the process-wide telemetry handle. Called from `logging::init`
/// after the global meter provider is installed.
pub fn init(enabled: bool) {
    let telemetry = if enabled {
        Telemetry::from_meter(opentelemetry::global::meter("piko-hostd"))
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
    prompt_inputs: Mutex<HashMap<(String, String), Vec<piko_protocol::ModelInputDebugSnapshot>>>,
    model_step_duration_ms: Option<Histogram<f64>>,
    model_step_calls: Option<Counter<u64>>,
    model_ttft_ms: Option<Histogram<f64>>,
    model_tokens: Option<Counter<u64>>,
    model_cost_usd: Option<Counter<f64>>,
    model_retries: Option<Counter<u64>>,
    model_fallbacks: Option<Counter<u64>>,
    tool_duration_ms: Option<Histogram<f64>>,
    tool_calls: Option<Counter<u64>>,
    turn_duration_ms: Option<Histogram<f64>>,
    turn_calls: Option<Counter<u64>>,
    turn_tokens: Option<Counter<u64>>,
    turn_cost_usd: Option<Counter<f64>>,
}

impl Telemetry {
    fn disabled() -> Self {
        Self {
            prompt_inputs: Mutex::new(HashMap::new()),
            model_step_duration_ms: None,
            model_step_calls: None,
            model_ttft_ms: None,
            model_tokens: None,
            model_cost_usd: None,
            model_retries: None,
            model_fallbacks: None,
            tool_duration_ms: None,
            tool_calls: None,
            turn_duration_ms: None,
            turn_calls: None,
            turn_tokens: None,
            turn_cost_usd: None,
        }
    }

    fn from_meter(meter: Meter) -> Self {
        Self {
            prompt_inputs: Mutex::new(HashMap::new()),
            model_step_duration_ms: Some(
                meter.f64_histogram("piko.model.step.duration_ms").build(),
            ),
            model_step_calls: Some(meter.u64_counter("piko.model.step.calls").build()),
            model_ttft_ms: Some(meter.f64_histogram("piko.model.ttft_ms").build()),
            model_tokens: Some(meter.u64_counter("piko.model.tokens").build()),
            model_cost_usd: Some(meter.f64_counter("piko.model.cost_usd").build()),
            model_retries: Some(meter.u64_counter("piko.model.retries").build()),
            model_fallbacks: Some(meter.u64_counter("piko.model.streaming_fallbacks").build()),
            tool_duration_ms: Some(meter.f64_histogram("piko.tool.duration_ms").build()),
            tool_calls: Some(meter.u64_counter("piko.tool.calls").build()),
            turn_duration_ms: Some(meter.f64_histogram("piko.turn.duration_ms").build()),
            turn_calls: Some(meter.u64_counter("piko.turn.calls").build()),
            turn_tokens: Some(meter.u64_counter("piko.turn.tokens").build()),
            turn_cost_usd: Some(meter.f64_counter("piko.turn.cost_usd").build()),
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
        if usage.cost.total != 0.0
            && let Some(counter) = &self.turn_cost_usd
        {
            counter.add(
                usage.cost.total,
                &[KeyValue::new("status", status.to_string())],
            );
        }
    }

    fn model_provider(model: &str, provider: &str) -> Vec<KeyValue> {
        vec![
            KeyValue::new("model", model.to_string()),
            KeyValue::new("provider", provider.to_string()),
        ]
    }

    pub fn clear_model_inputs(&self, session_id: &str, agent_instance_id: &str) {
        self.prompt_inputs
            .lock()
            .unwrap()
            .remove(&(session_id.to_string(), agent_instance_id.to_string()));
    }

    pub fn model_inputs(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Vec<piko_protocol::ModelInputDebugSnapshot> {
        self.prompt_inputs
            .lock()
            .unwrap()
            .get(&(session_id.to_string(), agent_instance_id.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

impl GatewayTelemetry for Telemetry {
    fn record_model_input(&self, input: piko_protocol::ModelInputDebugSnapshot) {
        const MAX_MODEL_INPUTS: usize = 32;
        let key = (input.session_id.clone(), input.agent_instance_id.clone());
        let mut inputs = self.prompt_inputs.lock().unwrap();
        let records = inputs.entry(key).or_default();
        records.push(input);
        if records.len() > MAX_MODEL_INPUTS {
            records.remove(0);
        }
    }

    fn record_ttft(&self, model: &str, provider: &str, ttft_ms: u64) {
        if let Some(histogram) = &self.model_ttft_ms {
            histogram.record(ttft_ms as f64, &Self::model_provider(model, provider));
        }
    }

    fn record_usage(&self, model: &str, provider: &str, usage: &Usage, cost_usd: f64) {
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
        if let Some(counter) = &self.model_cost_usd {
            counter.add(cost_usd, &Self::model_provider(model, provider));
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
