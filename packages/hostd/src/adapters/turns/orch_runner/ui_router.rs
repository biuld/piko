use std::collections::HashMap;

use tokio::sync::mpsc::UnboundedSender;

use crate::api::ServerMessage;

struct UiRoute {
    target_agent_instance_id: String,
    fallback: bool,
    tx: UnboundedSender<ServerMessage>,
}

/// Session- and operation-scoped delivery for approval, interaction, and
/// Agent projection events. A new command can never replace another Session's
/// sender.
#[derive(Default)]
pub(super) struct UiEventRouter {
    routes: std::sync::Mutex<HashMap<String, HashMap<String, UiRoute>>>,
}

impl UiEventRouter {
    pub(super) fn register(
        &self,
        session_id: &str,
        operation_id: &str,
        target_agent_instance_id: &str,
        fallback: bool,
        tx: UnboundedSender<ServerMessage>,
    ) {
        self.routes
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .insert(
                operation_id.to_string(),
                UiRoute {
                    target_agent_instance_id: target_agent_instance_id.to_string(),
                    fallback,
                    tx,
                },
            );
    }

    pub(super) fn unregister(&self, session_id: &str, operation_id: &str) {
        let mut routes = self.routes.lock().unwrap();
        let Some(session) = routes.get_mut(session_id) else {
            return;
        };
        session.remove(operation_id);
        if session.is_empty() {
            routes.remove(session_id);
        }
    }

    pub(super) fn has_route(&self, session_id: &str, agent_instance_id: &str) -> bool {
        self.matching_senders(session_id, agent_instance_id)
            .next()
            .is_some()
    }

    pub(super) fn publish(&self, session_id: &str, agent_instance_id: &str, event: ServerMessage) {
        for tx in self.matching_senders(session_id, agent_instance_id) {
            let _ = tx.send(event.clone());
        }
    }

    fn matching_senders(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> impl Iterator<Item = UnboundedSender<ServerMessage>> {
        let routes = self.routes.lock().unwrap();
        let mut exact = Vec::new();
        let mut fallback = Vec::new();
        if let Some(session) = routes.get(session_id) {
            for route in session.values() {
                if route.target_agent_instance_id == agent_instance_id {
                    exact.push(route.tx.clone());
                } else if route.fallback {
                    fallback.push(route.tx.clone());
                }
            }
        }
        if exact.is_empty() {
            fallback.into_iter()
        } else {
            exact.into_iter()
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    #[test]
    fn routes_exact_agent_before_session_fallback() {
        let router = UiEventRouter::default();
        let (root_tx, mut root_rx) = unbounded_channel();
        let (child_tx, mut child_rx) = unbounded_channel();
        router.register("s", "turn", "root", true, root_tx);
        router.register("s", "run", "child", false, child_tx);
        router.publish(
            "s",
            "child",
            ServerMessage::SessionCleared(piko_protocol::SessionClearedEvent {
                previous_session_id: "s".into(),
            }),
        );
        assert!(child_rx.try_recv().is_ok());
        assert!(root_rx.try_recv().is_err());
    }

    #[test]
    fn never_crosses_session_boundaries() {
        let router = UiEventRouter::default();
        let (first_tx, mut first_rx) = unbounded_channel();
        let (second_tx, mut second_rx) = unbounded_channel();
        router.register("first", "one", "root-first", true, first_tx);
        router.register("second", "two", "root-second", true, second_tx);
        router.publish(
            "first",
            "root-first",
            ServerMessage::SessionCleared(piko_protocol::SessionClearedEvent {
                previous_session_id: "first".into(),
            }),
        );
        assert!(first_rx.try_recv().is_ok());
        assert!(second_rx.try_recv().is_err());
    }
}
