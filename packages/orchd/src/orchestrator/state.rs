// ---- Orchestrator: state — snapshot, subscribe, get_graph, update_plan ----

use std::sync::Arc;

use crate::protocol::events::HostEvent;
use crate::protocol::runtime::{GraphEdge, GraphNode, GraphSnapshot};
use crate::protocol::state::OrchState;

use super::core::OrchCore;

/// Snapshot current orchestrator state.
pub async fn snapshot(core: &OrchCore) -> OrchState {
    let events = core.sourcing_events().await;
    let mut state = crate::protocol::event_store::rebuild_state(&events);
    state.run_id = core.run_id.clone();
    state.tool_sets = core.tool_registry.list_tool_sets().await;
    state
}

/// Subscribe to host events. Returns a cleanup function.
pub async fn subscribe(
    core: &OrchCore,
    listener: Box<dyn Fn(HostEvent) + Send + Sync>,
) -> Box<dyn FnOnce() + Send> {
    // Wrap listener to accept Value and deserialize to HostEvent
    let wrapped: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
        Arc::new(move |val: serde_json::Value| {
            if let Ok(host_event) = serde_json::from_value::<HostEvent>(val) {
                listener(host_event);
            }
        });

    let id = core
        .next_listener_id
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    {
        let mut listeners = core.listeners.write().await;
        listeners.insert(id, wrapped);
    }

    let listeners_ref = Arc::clone(&core.listeners);
    Box::new(move || {
        tokio::spawn(async move {
            let mut listeners = listeners_ref.write().await;
            listeners.remove(&id);
        });
    })
}

/// Get a graph representation of the orchestrator state.
pub async fn get_graph(core: &OrchCore) -> GraphSnapshot {
    let specs = core.agent_specs.read().await;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for (id, spec) in specs.iter() {
        nodes.push(GraphNode {
            id: id.clone(),
            label: spec.name.clone(),
            kind: spec.role.clone(),
            status: Some("idle".into()),
        });
    }

    // Add orchestrator node
    nodes.push(GraphNode {
        id: "orch".into(),
        label: "Orchestrator".into(),
        kind: "orchestrator".into(),
        status: Some("running".into()),
    });

    // All agents connect from orchestrator
    for (id, _) in specs.iter() {
        edges.push(GraphEdge {
            from: "orch".into(),
            to: id.clone(),
            label: Some("spawns".into()),
        });
    }

    GraphSnapshot { nodes, edges }
}

/// Update the plan for an agent task (best-effort).
pub async fn update_plan(
    core: &OrchCore,
    agent_id: String,
    task_id: String,
    plan_value: Vec<serde_json::Value>,
) {
    // Emit plan_updated event
    let host_event = HostEvent::PlanUpdated {
        order: crate::protocol::events::HostOrderBase::default(),
        agent_id,
        task_id,
        plan: plan_value,
    };

    let listeners = core.listeners.read().await;
    for listener in listeners.values() {
        listener(serde_json::to_value(&host_event).unwrap_or_default());
    }
}
