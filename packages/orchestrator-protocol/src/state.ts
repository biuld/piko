// ---- Orchestrator snapshot state ----

import type { AgentRuntimeState, AgentTaskState } from "./agents.js";
import type { ToolSet } from "./tools.js";

export interface OrchState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, ToolSet>;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
}
