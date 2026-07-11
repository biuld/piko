use std::sync::Arc;

use piko_protocol::ServerMessage;

use piko_protocol::{DisplayEvent, LifecycleEvent, PersistEvent};

pub fn persist_events_from_server_message(event: &ServerMessage) -> Vec<Arc<PersistEvent>> {
    match event {
        ServerMessage::TaskLifecycle(task_event) => {
            vec![Arc::new(PersistEvent::TaskEventCommitted(
                task_event.clone(),
            ))]
        }
        ServerMessage::Persist(event) => vec![Arc::new(event.clone())],
        _ => Vec::new(),
    }
}

pub fn display_events_from_server_message(event: &ServerMessage) -> Vec<Arc<DisplayEvent>> {
    match event {
        ServerMessage::Display(display) => vec![Arc::new(display.clone())],
        _ => Vec::new(),
    }
}

pub fn lifecycle_events_from_server_message(event: &ServerMessage) -> Vec<Arc<LifecycleEvent>> {
    match event {
        ServerMessage::TaskLifecycle(event) => vec![Arc::new(LifecycleEvent::Task(event.clone()))],
        ServerMessage::TurnLifecycle(event) => vec![Arc::new(LifecycleEvent::Turn(event.clone()))],
        _ => Vec::new(),
    }
}

pub fn server_message_from_persist_event(event: &PersistEvent) -> Option<ServerMessage> {
    match event {
        PersistEvent::TaskEventCommitted(event) => {
            Some(ServerMessage::TaskLifecycle(event.clone()))
        }
        _ => None,
    }
}

pub fn server_message_from_display_event(event: &DisplayEvent) -> Option<ServerMessage> {
    match event {
        DisplayEvent::ToolCallDelta { .. } => None,
        _ => Some(ServerMessage::Display(event.clone())),
    }
}
