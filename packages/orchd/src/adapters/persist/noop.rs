use std::sync::Mutex;

use async_trait::async_trait;

use crate::integration::{MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit};

/// Immediate-ack sink for tests and runs without durable storage.
#[derive(Debug, Default)]
pub struct NoopPersistSink {
    next_seq: Mutex<u64>,
}

#[async_trait]
impl PersistSink for NoopPersistSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        Ok(PersistAck {
            session_id: event.session_id,
            task_id: event.task_id,
            message_id: Some(event.message_id),
            task_seq: event.task_seq,
        })
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        Ok(PersistAck {
            session_id: event.session_id,
            task_id: event.task_id,
            message_id: None,
            task_seq: event.task_seq,
        })
    }
}

impl NoopPersistSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_task_seq(&self, last: u64) -> u64 {
        let mut seq = self.next_seq.lock().expect("noop persist sink lock");
        let next = last.max(*seq) + 1;
        *seq = next;
        next
    }
}
