// ---- Orchestrator command/response protocol envelopes ----
// Protocol-level command and response types for remote orchestrator communication.

import type { AgentSpec, AgentTask, AgentTaskId } from "./agents.js";
import type { OrchModelConfig, OrchRunCommandOptions, OrchRunResult } from "./runtime.js";
import type { OrchState } from "./state.js";
import type { ToolSet } from "./tools.js";

export type OrchestratorCommand =
  | { type: "register_agent"; spec: AgentSpec }
  | { type: "unregister_agent"; agentId: string }
  | { type: "register_tool_set"; toolSet: ToolSet }
  | { type: "unregister_tool_set"; toolSetId: string }
  | { type: "set_model_config"; config: OrchModelConfig }
  | { type: "dispatch"; task: AgentTask }
  | { type: "run"; prompt: string; options?: OrchRunCommandOptions }
  | { type: "cancel_task"; taskId: AgentTaskId; reason?: string }
  | { type: "snapshot" };

export type OrchestratorResponse =
  | { type: "ok"; value?: unknown }
  | { type: "error"; code: string; message: string }
  | { type: "task_dispatched"; taskId: AgentTaskId }
  | { type: "run_result"; result: OrchRunResult }
  | { type: "snapshot"; state: OrchState };
