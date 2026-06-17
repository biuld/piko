import type { OrchState } from "piko-orchestrator-protocol";
import type { StateActorState } from "./types.js";

export interface GraphNode {
  id: string;
  label: string;
  kind: string;
  status?: string;
}

export interface GraphEdge {
  from: string;
  to: string;
  label?: string;
}

export function buildSnapshot(state: StateActorState): OrchState {
  return {
    runId: state.runId,
    status: state.status,
    toolSets: state.toolSets,
    agents: state.agents,
    tasks: state.tasks,
  };
}

export function buildGraph(state: StateActorState): {
  nodes: GraphNode[];
  edges: GraphEdge[];
} {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  for (const [id, agent] of Object.entries(state.agents)) {
    nodes.push({
      id: `agent:${id}`,
      label: agent.spec.name,
      kind: "agent",
      status: agent.status,
    });
    if (agent.activeTaskId) {
      edges.push({
        from: `agent:${id}`,
        to: `task:${agent.activeTaskId}`,
        label: "owns",
      });
    }
  }

  for (const [id, task] of Object.entries(state.tasks)) {
    nodes.push({
      id: `task:${id}`,
      label: task.prompt.slice(0, 50),
      kind: "task",
      status: task.status,
    });
    if (task.parentTaskId) {
      edges.push({
        from: `task:${task.parentTaskId}`,
        to: `task:${id}`,
        label: "parent",
      });
    }
  }

  return { nodes, edges };
}
