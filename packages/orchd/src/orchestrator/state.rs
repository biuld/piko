// ---- Orchestrator: state — snapshot, subscribe, get_graph, update_plan ----

use crate::protocol::agents::{AgentRuntimeState, AgentStatus};
use crate::protocol::runtime::{GraphEdge, GraphNode, GraphSnapshot};
use crate::protocol::state::OrchState;

use super::core::OrchCore;

/// Snapshot current orchestrator state.
pub async fn snapshot(core: &OrchCore) -> OrchState {
    let specs = core.agent_specs.read().await;
    let tasks = core.task_states.read().await.clone();
    let agents = specs
        .iter()
        .map(|(id, spec)| {
            let active_task_id = tasks
                .values()
                .find(|task| {
                    task.target_agent_id == *id
                        && task.status == crate::protocol::agents::AgentTaskStatus::Running
                })
                .map(|task| task.id.clone());
            let status = if active_task_id.is_some() {
                AgentStatus::Running
            } else {
                AgentStatus::Idle
            };

            (
                id.clone(),
                AgentRuntimeState {
                    id: id.clone(),
                    spec: spec.clone(),
                    status,
                    active_task_id,
                    transcript: Vec::new(),
                },
            )
        })
        .collect();

    let mut state = OrchState::new(core.run_id.clone());
    state.tool_sets = core.tool_registry.list_tool_sets().await;
    state.agents = agents;
    state.tasks = tasks;
    state
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
    _core: &OrchCore,
    _agent_id: String,
    _task_id: String,
    _plan_value: Vec<serde_json::Value>,
) {
    // Unified Event intentionally has no plan event. Plan state needs a
    // dedicated host-visible contract before being reintroduced.
}
