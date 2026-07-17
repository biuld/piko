use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::agent_runtime::{SessionEvent, SessionEventEnvelope};

struct ObservationRoute {
    target_agent_instance_id: String,
    fallback: bool,
    hub: Arc<piko_orchd::events::SessionOutputHub>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_agent_route_wins_over_session_fallback() {
        let router = SessionObservationRouter::default();
        let root = Arc::new(piko_orchd::events::SessionOutputHub::new(
            "s".into(),
            "root".into(),
            4,
        ));
        let child = Arc::new(piko_orchd::events::SessionOutputHub::new(
            "s".into(),
            "child".into(),
            4,
        ));
        router.register("s", "turn", "root", true, root);
        router.register("s", "run", "child", false, child);

        assert_eq!(
            router.hub_for("s", "child").unwrap().cursor().epoch,
            "child"
        );
        assert_eq!(router.hub_for("s", "other").unwrap().cursor().epoch, "root");
        assert!(router.hub_for("other", "child").is_none());
    }
}

/// Session-scoped routing for reliable product observation. The route owns no
/// queue: it selects the authoritative SessionOutputHub contract.
#[derive(Default)]
pub(super) struct SessionObservationRouter {
    routes: std::sync::Mutex<HashMap<String, HashMap<String, ObservationRoute>>>,
}

impl SessionObservationRouter {
    pub(super) fn register(
        &self,
        session_id: &str,
        operation_id: &str,
        target_agent_instance_id: &str,
        fallback: bool,
        hub: Arc<piko_orchd::events::SessionOutputHub>,
    ) {
        self.routes
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .insert(
                operation_id.to_string(),
                ObservationRoute {
                    target_agent_instance_id: target_agent_instance_id.to_string(),
                    fallback,
                    hub,
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
        !self.matching_hubs(session_id, agent_instance_id).is_empty()
    }

    pub(super) fn hub_for(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Option<Arc<piko_orchd::events::SessionOutputHub>> {
        self.matching_hubs(session_id, agent_instance_id)
            .into_iter()
            .next()
    }

    pub(super) async fn publish(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        agent_id: &str,
        event: SessionEvent,
    ) {
        for hub in self.matching_hubs(session_id, agent_instance_id) {
            let _ = hub
                .publish_event(SessionEventEnvelope {
                    agent_instance_id: agent_instance_id.to_string(),
                    agent_id: agent_id.to_string(),
                    cursor: hub.cursor(),
                    event: event.clone(),
                })
                .await;
        }
    }

    fn matching_hubs(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Vec<Arc<piko_orchd::events::SessionOutputHub>> {
        let routes = self.routes.lock().unwrap();
        let mut exact = Vec::new();
        let mut fallback = Vec::new();
        if let Some(session) = routes.get(session_id) {
            for route in session.values() {
                let targets = if route.target_agent_instance_id == agent_instance_id {
                    &mut exact
                } else if route.fallback {
                    &mut fallback
                } else {
                    continue;
                };
                if !targets.iter().any(|hub| Arc::ptr_eq(hub, &route.hub)) {
                    targets.push(Arc::clone(&route.hub));
                }
            }
        }
        if exact.is_empty() { fallback } else { exact }
    }
}
