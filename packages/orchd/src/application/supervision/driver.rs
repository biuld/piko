use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt;

use super::supervisor::SupervisorState;

pub(crate) fn spawn_task_runtime<F>(
    state: Arc<SupervisorState>,
    task_id: String,
    generation: u64,
    runtime: F,
) where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        // Catch panics so cleanup still runs and the failure is visible in logs.
        // (Byte-index string truncation used to panic here on CJK/emoji summaries.)
        if let Err(payload) = AssertUnwindSafe(runtime).catch_unwind().await {
            let message = panic_payload_message(&payload);
            tracing::error!(
                task_id = %task_id,
                generation,
                panic = %message,
                "task runtime panicked; cleaning up"
            );
        }
        state
            .registry
            .cleanup_runtime_generation(&task_id, generation)
            .await;
    });
}

fn panic_payload_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
