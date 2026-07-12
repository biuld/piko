use std::sync::Mutex;

use async_trait::async_trait;

use orchd_api::{MessageCommit, PersistAck, PersistError, PersistSink, TaskShardEnsure};

/// Test sink that records message commits and returns immediate acks.
#[derive(Debug, Default)]
pub struct CollectingPersistSink {
    messages: Mutex<Vec<MessageCommit>>,
    next_seq: Mutex<u64>,
}

#[async_trait]
impl PersistSink for CollectingPersistSink {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        self.messages
            .lock()
            .expect("collecting sink lock")
            .push(event.clone());
        Ok(PersistAck {
            session_id: event.session_id,
            task_id: event.task_id,
            message_id: Some(event.message_id),
            task_seq: event.task_seq,
        })
    }

    async fn ensure_task_shard(&self, ensure: TaskShardEnsure) -> Result<PersistAck, PersistError> {
        Ok(PersistAck {
            session_id: ensure.session_id,
            task_id: ensure.task_id,
            message_id: None,
            task_seq: 0,
        })
    }
}

impl CollectingPersistSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn messages(&self) -> Vec<MessageCommit> {
        self.messages.lock().expect("collecting sink lock").clone()
    }

    pub fn next_task_seq(&self, last: u64) -> u64 {
        let mut seq = self.next_seq.lock().expect("collecting sink lock");
        let next = last.max(*seq) + 1;
        *seq = next;
        next
    }
}
