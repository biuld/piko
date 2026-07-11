use std::sync::Arc;

use orchd_api::PersistSink;
use tokio::sync::RwLock;

/// Supervisor-owned persist sink slot shared with long-lived task runtimes.
#[derive(Clone)]
pub(crate) struct SharedPersistSink {
    inner: Arc<RwLock<Option<Arc<dyn PersistSink>>>>,
}

impl SharedPersistSink {
    pub(crate) fn new(inner: Arc<RwLock<Option<Arc<dyn PersistSink>>>>) -> Self {
        Self { inner }
    }

    pub(crate) async fn resolve(&self) -> Result<Arc<dyn PersistSink>, String> {
        self.inner
            .read()
            .await
            .clone()
            .ok_or_else(|| "persistence unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::CollectingPersistSink;

    #[tokio::test]
    async fn resolve_follows_supervisor_rebind() {
        let cell = Arc::new(RwLock::new(None));
        let shared = SharedPersistSink::new(Arc::clone(&cell));
        let sink_a = Arc::new(CollectingPersistSink::new()) as Arc<dyn PersistSink>;
        let sink_b = Arc::new(CollectingPersistSink::new()) as Arc<dyn PersistSink>;

        *cell.write().await = Some(Arc::clone(&sink_a));
        let resolved_a = shared.resolve().await.expect("sink a");
        assert!(Arc::ptr_eq(&resolved_a, &sink_a));

        *cell.write().await = Some(Arc::clone(&sink_b));
        let resolved_b = shared.resolve().await.expect("sink b");
        assert!(Arc::ptr_eq(&resolved_b, &sink_b));
    }
}
