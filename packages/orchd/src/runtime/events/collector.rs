use std::sync::Arc;

use orchd_api::RealtimeDeltaSink;
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

struct RealtimeCollector {
    events: std::sync::Mutex<Vec<RealtimeFrame>>,
    sink: Option<Arc<dyn RealtimeDeltaSink>>,
}

#[derive(Clone)]
pub(crate) struct SharedRealtimeCollector(Arc<RealtimeCollector>);

impl Default for SharedRealtimeCollector {
    fn default() -> Self {
        Self::with_sink(None)
    }
}

impl SharedRealtimeCollector {
    pub(crate) fn with_sink(sink: Option<Arc<dyn RealtimeDeltaSink>>) -> Self {
        Self(Arc::new(RealtimeCollector {
            events: std::sync::Mutex::new(Vec::new()),
            sink,
        }))
    }

    pub(crate) fn take(&self) -> Vec<RealtimeFrame> {
        let mut events = self.0.events.lock().expect("realtime collector poisoned");
        std::mem::take(&mut *events)
    }

    pub(crate) fn push(&self, frame: RealtimeFrame) {
        let delta_seq = {
            let mut events = self.0.events.lock().expect("realtime collector poisoned");
            let delta_seq = events.len() as u64;
            events.push(frame.clone());
            delta_seq
        };

        if let Some(sink) = &self.0.sink {
            sink.try_publish(piko_protocol::agent_runtime::RealtimeDeltaEnvelope {
                agent_instance_id: frame.agent_instance_id,
                execution_id: frame.execution_id,
                agent_id: frame.agent_id,
                message_id: Some(frame.message_id),
                delta_seq,
                delta: frame.delta,
            });
        }
    }
}
