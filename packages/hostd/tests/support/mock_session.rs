use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use orchd_api::{SessionOutputStream, SessionStreamError, SessionSubscription};
use piko_protocol::agent_runtime::{
    SessionCursor, SessionEvent, SessionEventEnvelope, SessionOutput, SessionOutputEnvelope,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub struct MockSessionPublisher {
    session_id: String,
    epoch: String,
    cursor_seq: AtomicU64,
    tx: mpsc::UnboundedSender<Result<SessionOutputEnvelope, SessionStreamError>>,
}

impl MockSessionPublisher {
    pub fn new(session_id: impl Into<String>) -> (Arc<Self>, SessionSubscription) {
        let session_id = session_id.into();
        let epoch = format!("mock_{}", uuid::Uuid::new_v4());
        let cursor = SessionCursor {
            epoch: epoch.clone(),
            seq: 0,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let publisher = Arc::new(Self {
            session_id: session_id.clone(),
            epoch,
            cursor_seq: AtomicU64::new(0),
            tx,
        });
        let output: SessionOutputStream = Box::pin(UnboundedReceiverStream::new(rx));
        let subscription = SessionSubscription {
            session_id,
            cursor,
            output,
        };
        (publisher, subscription)
    }

    pub fn publish(
        &self,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        task_seq: u64,
        event: SessionEvent,
    ) {
        let seq = self.cursor_seq.fetch_add(1, Ordering::Relaxed) + 1;
        let envelope = SessionEventEnvelope {
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            task_seq,
            cursor: SessionCursor {
                epoch: self.epoch.clone(),
                seq,
            },
            event,
        };
        let _ = self.tx.send(Ok(SessionOutputEnvelope {
            session_id: self.session_id.clone(),
            emitted_at: chrono::Utc::now().timestamp_millis(),
            output: SessionOutput::Event(envelope),
        }));
    }
}
