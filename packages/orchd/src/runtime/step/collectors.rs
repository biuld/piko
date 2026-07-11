use std::sync::Arc;

use piko_protocol::Message;

use piko_protocol::{DisplayEvent, PersistEvent};

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
pub(crate) struct SharedDisplayCollector(Arc<std::sync::Mutex<Vec<DisplayEvent>>>);

impl SharedDisplayCollector {
    pub(crate) fn take(&self) -> Vec<DisplayEvent> {
        let mut events = self.0.lock().expect("display collector poisoned");
        std::mem::take(&mut *events)
    }

    pub(crate) fn push(&self, event: DisplayEvent) {
        let mut events = self.0.lock().expect("display collector poisoned");
        events.push(event);
    }
}
