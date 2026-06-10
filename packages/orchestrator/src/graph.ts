import type {
  OrchestratorGraph,
  OrchestratorGraphEdge,
  OrchestratorGraphNode,
  OrchestratorState,
} from "piko-orchestrator-protocol";

/**
 * Render a graph projection from the current orchestrator state.
 * This can be used by the TUI for a team/agent visualization.
 */
export function renderGraph(state: OrchestratorState): OrchestratorGraph {
  const nodes: OrchestratorGraphNode[] = [];
  const edges: OrchestratorGraphEdge[] = [];

  // Agent nodes
  for (const agent of Object.values(state.agents)) {
    nodes.push({
      id: agent.id,
      kind: "agent",
      status: agent.status,
      label: agent.spec.name,
      metadata: {
        role: agent.spec.role,
        toolSetIds: agent.spec.toolSetIds,
        activeTaskId: agent.activeTaskId,
      },
    });
  }

  // Task nodes
  for (const task of Object.values(state.tasks)) {
    nodes.push({
      id: task.id,
      kind: "task",
      status: task.status,
      label: task.prompt.slice(0, 80),
      metadata: {
        targetAgentId: task.targetAgentId,
        priority: task.priority,
        parentTaskId: task.parentTaskId,
      },
    });

    // Edge: task -> agent (assigned_to)
    edges.push({
      from: task.id,
      to: task.targetAgentId,
      kind: "assigned_to",
    });

    // Edge: parent task -> child task (spawned)
    if (task.parentTaskId) {
      edges.push({
        from: task.parentTaskId,
        to: task.id,
        kind: "spawned",
      });
    }

    // Edge: task -> approval (waiting_for)
    for (const approval of Object.values(state.approvals)) {
      if (approval.taskId === task.id) {
        edges.push({
          from: task.id,
          to: approval.id,
          kind: "waiting_for",
        });
      }
    }
  }

  // Watch nodes
  for (const watch of Object.values(state.watches)) {
    nodes.push({
      id: watch.id,
      kind: "watch",
      status: watch.active ? "active" : "inactive",
      label: `${watch.kind} watch`,
      metadata: {
        agentId: watch.agentId,
        kind: watch.kind,
      },
    });
    edges.push({
      from: watch.id,
      to: watch.agentId,
      kind: "triggered",
    });
  }

  // Lock nodes
  for (const lock of Object.values(state.locks)) {
    nodes.push({
      id: lock.id,
      kind: "lock",
      status: lock.holderAgentId ? "held" : "free",
      label: `${lock.resource} (${lock.mode})`,
      metadata: {
        holderAgentId: lock.holderAgentId,
        queue: lock.queue.length,
      },
    });

    if (lock.holderAgentId) {
      edges.push({
        from: lock.holderAgentId,
        to: lock.id,
        kind: "requires",
      });
      if (lock.holderTaskId) {
        edges.push({
          from: lock.holderTaskId,
          to: lock.id,
          kind: "blocked_by",
        });
      }
    }

    // Queue waiters
    for (const waiter of lock.queue) {
      edges.push({
        from: waiter.agentId,
        to: lock.id,
        kind: "waiting_for",
      });
    }
  }

  // Approval nodes
  for (const approval of Object.values(state.approvals)) {
    nodes.push({
      id: approval.id,
      kind: "approval",
      status: approval.status,
      label: `Approval for ${approval.agentId}`,
    });
  }

  return { nodes, edges };
}
