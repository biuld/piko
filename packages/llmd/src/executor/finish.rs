use super::*;

pub(super) fn write_model_step_finish(
    capture: &ModelStepCapture,
    retries: Vec<TrajectoryRetryAttempt>,
    fallback: Option<TrajectoryFallback>,
    usage: Option<Usage>,
    finished_at: i64,
) {
    capture
        .telemetry
        .record_model_step(TrajectoryModelStepRecord {
            identity: capture.identity.clone(),
            step_id: capture.step_id.clone(),
            provider: capture.provider.clone(),
            model: capture.model.clone(),
            request: capture.request.clone(),
            options: capture.options.clone(),
            started_at: capture.started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(finished_at.saturating_sub(capture.started_at) as u64),
            retries,
            fallback,
            response: None,
            message_id: Some(capture.message_id.clone()),
            usage: usage.map(Box::new),
        });
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
