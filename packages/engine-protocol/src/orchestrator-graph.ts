// ---- Orchestrator graph projection ----

export interface OrchestratorGraphNode {
  id: string;
  kind: "agent" | "task" | "watch" | "lock" | "approval" | "artifact";
  status: string;
  label: string;
  metadata?: Record<string, unknown>;
}

export interface OrchestratorGraphEdge {
  from: string;
  to: string;
  kind:
    | "assigned_to"
    | "triggered"
    | "waiting_for"
    | "blocked_by"
    | "spawned"
    | "produced"
    | "requires";
}

export interface OrchestratorGraph {
  nodes: OrchestratorGraphNode[];
  edges: OrchestratorGraphEdge[];
}
