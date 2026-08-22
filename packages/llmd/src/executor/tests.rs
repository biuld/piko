use std::sync::Mutex;

use futures::stream;
use piko_protocol::Usage;

use super::*;

#[derive(Default)]
struct RecordingTelemetry {
    finished: Mutex<Option<TrajectoryModelStepRecord>>,
}

impl crate::telemetry::GatewayTelemetry for RecordingTelemetry {
    fn record_model_step(&self, record: piko_protocol::TrajectoryModelStepRecord) {
        *self.finished.lock().unwrap() = Some(record);
    }

    fn record_ttft(&self, _model: &str, _provider: &str, _ttft_ms: u64) {}

    fn record_usage(&self, _model: &str, _provider: &str, _usage: &Usage) {}

    fn record_retry(&self, _model: &str, _provider: &str, _error_class: &str, _attempt: u32) {}

    fn record_fallback(&self, _model: &str, _provider: &str) {}
}

fn capture(telemetry: Arc<RecordingTelemetry>) -> ModelStepCapture {
    ModelStepCapture {
        telemetry: telemetry as Arc<dyn crate::telemetry::GatewayTelemetry>,
        identity: TrajectoryIdentity {
            session_id: "s".into(),
            agent_instance_id: "a".into(),
            run_id: "r".into(),
            execution_id: None,
            source_turn_id: None,
        },
        step_id: "step-1".into(),
        provider: "test".into(),
        model: "test-model".into(),
        request: serde_json::json!({}),
        options: serde_json::json!({}),
        started_at: 1,
        message_id: "message-1".into(),
    }
}

#[tokio::test]
async fn finish_record_carries_final_step_usage() {
    let telemetry = Arc::new(RecordingTelemetry::default());
    let usage = Usage {
        input: 100,
        output: 20,
        cache_read: 80,
        cache_write: 20,
        total_tokens: 120,
        units: Default::default(),
        cost: Default::default(),
    };
    let input = stream::iter([
        InferenceEvent::Usage(usage.clone()),
        InferenceEvent::Completed(FinishReason::Completed {
            reason: "end_turn".into(),
        }),
    ]);
    let mut wrapped = Box::pin(wrap_model_step_finish(
        input,
        capture(Arc::clone(&telemetry)),
        Vec::new(),
        None,
    ));
    while wrapped.next().await.is_some() {}

    let record = telemetry
        .finished
        .lock()
        .unwrap()
        .clone()
        .expect("finish record written");
    assert_eq!(record.usage.as_deref(), Some(&usage));
    assert_eq!(record.message_id.as_deref(), Some("message-1"));
    assert!(record.finished_at.is_some());
    assert_eq!(record.step_id, "step-1");
}

#[tokio::test]
async fn abandoned_stream_writes_no_finish_record() {
    let telemetry = Arc::new(RecordingTelemetry::default());
    let input = stream::iter([InferenceEvent::Usage(Usage {
        input: 10,
        output: 0,
        cache_read: 0,
        cache_write: 10,
        total_tokens: 10,
        units: Default::default(),
        cost: Default::default(),
    })]);
    let wrapped = wrap_model_step_finish(input, capture(Arc::clone(&telemetry)), Vec::new(), None);
    drop(wrapped);

    assert!(telemetry.finished.lock().unwrap().is_none());
}

#[tokio::test]
async fn completed_event_flushes_finish_before_consumer_stops() {
    let telemetry = Arc::new(RecordingTelemetry::default());
    let usage = Usage {
        input: 50,
        output: 10,
        cache_read: 45,
        cache_write: 5,
        total_tokens: 60,
        units: Default::default(),
        cost: Default::default(),
    };
    let input = stream::iter([
        InferenceEvent::Usage(usage.clone()),
        InferenceEvent::Completed(FinishReason::Completed {
            reason: "end_turn".into(),
        }),
    ]);
    let mut wrapped = Box::pin(wrap_model_step_finish(
        input,
        capture(Arc::clone(&telemetry)),
        Vec::new(),
        None,
    ));

    while let Some(event) = wrapped.next().await {
        if matches!(event, InferenceEvent::Completed(_)) {
            break;
        }
    }
    drop(wrapped);

    let record = telemetry
        .finished
        .lock()
        .unwrap()
        .clone()
        .expect("finish record flushed before consumer stopped");
    assert_eq!(record.usage.as_deref(), Some(&usage));
    assert!(record.finished_at.is_some());
    assert!(record.duration_ms.is_some());
}
