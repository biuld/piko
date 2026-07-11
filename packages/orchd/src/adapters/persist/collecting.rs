use std::sync::Mutex;

use async_trait::async_trait;

use crate::integration::{MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit};

/// Test sink that records commits and returns immediate acks.
#[derive(Debug, Default)]
pub struct CollectingPersistSink {
    messages: Mutex<Vec<MessageCommit>>,
    task_events: Mutex<Vec<TaskEventCommit>>,
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

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        self.task_events
            .lock()
            .expect("collecting sink lock")
            .push(event.clone());
        Ok(PersistAck {
            session_id: event.session_id,
            task_id: event.task_id,
            message_id: None,
            task_seq: event.task_seq,
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

    pub fn task_events(&self) -> Vec<TaskEventCommit> {
        self.task_events
            .lock()
            .expect("collecting sink lock")
            .clone()
    }

    pub fn next_task_seq(&self, last: u64) -> u64 {
        let mut seq = self.next_seq.lock().expect("collecting sink lock");
        let next = last.max(*seq) + 1;
        *seq = next;
        next
    }
}
