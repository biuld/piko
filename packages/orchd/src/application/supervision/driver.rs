use std::future::Future;
use std::sync::Arc;

use super::supervisor::SupervisorState;

pub(crate) fn spawn_task_runtime<F>(state: Arc<SupervisorState>, task_id: String, runtime: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        runtime.await;
        state.registry.cleanup_runtime(&task_id).await;
    });
}
