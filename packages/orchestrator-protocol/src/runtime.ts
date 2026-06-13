// ---- Orchestrator runtime config / result types ----

import type { Message, Model } from "./messages.js";
import type { ModelProviderConfig, ModelRunSettings } from "./model.js";

// ---- Model config passed by Host ----

export interface OrchModelConfig {
  model: Model<string>;
  provider: ModelProviderConfig;
  settings: ModelRunSettings;
}

// ---- Run options / result ----

/** Serializable run options for protocol command envelopes. */
export interface OrchRunCommandOptions {
  targetAgentId?: string;
}

/** Local runtime run options. */
export interface OrchRunOptions extends OrchRunCommandOptions {
  signal?: AbortSignal;
}

export interface OrchRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
}

// ---- Orchestrator interface (Host-facing contract) ----

import type { AgentSpec, AgentTask, AgentTaskId } from "./agents.js";
import type { ApprovalGateway } from "./approval.js";
import type { HostEventListener } from "./events.js";
import type { OrchState } from "./state.js";
import type { ToolProvider, ToolSet } from "./tools.js";

export interface Orchestrator {
  registerAgent(spec: AgentSpec): void;
  unregisterAgent(agentId: string): void;
  registerToolSet(toolSet: ToolSet): void;
  unregisterToolSet(toolSetId: string): void;
  setModelConfig(config: OrchModelConfig): void;
  setApprovalGateway(gateway: ApprovalGateway | undefined): void;
  registerProvider(provider: ToolProvider): void;
  dispatch(task: AgentTask): Promise<AgentTaskId>;
  run(prompt: string, opts?: OrchRunOptions): Promise<OrchRunResult>;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
}
