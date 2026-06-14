// ---- AgentActor — shared types ----
// AgentMsg, AgentRuntimeState, AgentActorDeps, and step-loop types.

import type {
  AgentSpec,
  AgentTask,
  AgentTaskResult,
  Message,
  ModelProviderConfig,
  ModelRunSettings,
} from "piko-orchestrator-protocol";
import type { ModelStepExecutor } from "../../model/types.js";
import type { ToolRegistry } from "../../tools/index.js";
import type { OrchestratorEvent } from "../state.js";

// ---- Messages ----

export type AgentMsg =
  | { type: "dispatch"; task: AgentTask }
  | { type: "cancel"; taskId: string; reason?: string }
  | {
      type: "wake";
      reason: { type: string; taskId?: string; approvalId?: string };
    }
  | {
      type: "set_model_config";
      config: {
        model?: { id: string; name?: string; provider?: string };
        provider?: Record<string, unknown>;
        settings?: { maxSteps?: number; allowToolCalls?: boolean; allowApprovals?: boolean };
      };
    }
  | { type: "runner_finished"; result: any }
  | { type: "runner_failed"; error: string };

// ---- Agent private state ----

export interface AgentRuntimeState {
  spec: AgentSpec;
  status: "idle" | "running" | "failed" | "stopped";
  currentTaskId?: string;
  transcript: Message[];
  engineState?: unknown;
  stepCount: number;
  cancelled: Set<string>;
  pendingReply?: import("../../kernel/envelope.js").Envelope;
  currentRunnerId?: string;
}

// ---- Dependencies ----

export interface AgentActorDeps {
  modelExecutor: ModelStepExecutor;
  emit: (event: OrchestratorEvent) => Promise<void>;
  maxSteps?: number;
  modelConfig?: {
    model: import("piko-orchestrator-protocol").Model<string>;
    provider: ModelProviderConfig;
    settings: ModelRunSettings;
  };
  actorSystem?: import("../../kernel/actor-system.js").ActorSystem;
  /** DI container for prototype ToolActor creation + discovery. */
  toolRegistry: ToolRegistry;
}

// ---- Step-loop types ----

/** Terminal result from a step (cancelled / error / aborted / completed / max_steps). */
export type StepTerminal = AgentTaskResult & {
  messages: Message[];
  totalSteps: number;
  finalStatus: string;
};

/** Outcome of a single step: either continue the loop, or return a terminal result. */
export type StepOutcome = { kind: "continue" } | { kind: "terminal"; result: StepTerminal };
