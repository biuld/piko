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
        state.tasks = self.state.registry.tasks_snapshot().await;
        state
    }

    pub async fn get_graph(&self) -> GraphSnapshot {
        let specs = self.state.agent_specs.read().await;
        let tasks = self.state.registry.tasks_snapshot().await;
        let task_dag = self.state.registry.task_dag_snapshot().await;
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
        for (task_id, task) in &tasks {
            nodes.push(GraphNode {
                id: task_id.clone(),
                label: format!("{} task", task.target_agent_id),
                kind: "task".into(),
                status: Some(format!("{:?}", task.status).to_lowercase()),
            });
            edges.push(GraphEdge {
                from: task.target_agent_id.clone(),
                to: task_id.clone(),
                label: Some("runs".into()),
            });
        }
        for (task_id, parent) in &task_dag {
            let from = parent.as_deref().unwrap_or("orch");
            edges.push(GraphEdge {
                from: from.into(),
                to: task_id.clone(),
                label: Some("spawns".into()),
            });
        }
        GraphSnapshot { nodes, edges }
    }
}
