use std::sync::Arc;

use piko_protocol::Message;

use piko_protocol::PersistEvent;

use crate::domain::RealtimeFrame;

#[derive(Clone, Default)]
pub(crate) struct SharedAssistantMessageCollector(Arc<std::sync::Mutex<Option<Message>>>);

impl SharedAssistantMessageCollector {
    pub(crate) fn take(&self) -> Message {
        self.0
            .lock()
            .expect("assistant message collector poisoned")
            .take()
            .expect("assistant message missing")
    }

    pub(crate) fn set(&self, message: Message) {
        *self.0.lock().expect("assistant message collector poisoned") = Some(message);
    }
}

#[derive(Clone, Default)]
pub(crate) struct SharedPersistCollector(Arc<std::sync::Mutex<Vec<PersistEvent>>>);

impl SharedPersistCollector {
    pub(crate) fn take(&self) -> Vec<PersistEvent> {
        let mut events = self.0.lock().expect("persist collector poisoned");
        std::mem::take(&mut *events)
    }

    pub(crate) fn push(&self, event: PersistEvent) {
        let mut events = self.0.lock().expect("persist collector poisoned");
        events.push(event);
    }
}

#[derive(Clone, Default)]
pub(crate) struct SharedRealtimeCollector(Arc<std::sync::Mutex<Vec<RealtimeFrame>>>);

impl SharedRealtimeCollector {
    pub(crate) fn take(&self) -> Vec<RealtimeFrame> {
        let mut events = self.0.lock().expect("realtime collector poisoned");
        std::mem::take(&mut *events)
    }

    pub(crate) fn push(&self, frame: RealtimeFrame) {
        let mut events = self.0.lock().expect("realtime collector poisoned");
        events.push(frame);
    }
}
