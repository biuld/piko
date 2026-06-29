// ---- Snapshot — state snapshot, graph, and stream management ----

use std::collections::HashMap;

use piko_protocol::AgentId;
use piko_protocol::agents::{AgentRuntimeState as ProtoAgentRuntimeState, AgentStatus};
use piko_protocol::runtime::{GraphEdge, GraphNode, GraphSnapshot};
use piko_protocol::state::OrchState;

use super::supervisor::Supervisor;

impl Supervisor {
    pub async fn snapshot(&self) -> OrchState {
        let specs = self.state.agent_specs.read().await;
        let agents = specs
            .iter()
            .map(|(id, spec)| {
                (
                    id.clone(),
                    ProtoAgentRuntimeState {
                        id: id.clone(),
                        spec: spec.clone(),
                        status: AgentStatus::Idle,
                        active_task_id: None,
                        transcript: Vec::new(),
                    },
                )
            })
            .collect::<HashMap<AgentId, ProtoAgentRuntimeState>>();
        let mut state = OrchState::new(self.state.run_id.clone());
        state.tool_sets = self.state.tool_registry.list_tool_sets().await;
        state.agents = agents;
        state
    }

    pub async fn get_graph(&self) -> GraphSnapshot {
        let specs = self.state.agent_specs.read().await;
        let dag = self.state.dag.read().await;
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
        nodes.push(GraphNode {
            id: "orch".into(),
            label: "Orchestrator".into(),
            kind: "orchestrator".into(),
            status: Some("running".into()),
        });
        for (id, parent) in dag.iter() {
            let from = parent.as_deref().unwrap_or("orch");
            edges.push(GraphEdge {
                from: from.into(),
                to: id.clone(),
                label: Some("spawns".into()),
            });
        }
        GraphSnapshot { nodes, edges }
    }
}
