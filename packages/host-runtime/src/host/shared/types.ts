import type {
  ImageContent,
  Message,
  ModelRunSettings,
  ModelStepExecutor,
  Orchestrator,
  ToolApprovalRequest,
} from "piko-orchestrator";
import type { HostConfig, ModelRegistry } from "../../models/index.js";
import type { PromptTemplate } from "../../prompts/index.js";
import type { CreateSessionRuntimeOptions } from "../../session/index.js";
import type { SettingsManager } from "../../settings/index.js";

export type { HostToolCallbacks } from "../../tools/host-provider.js";

import type { HostToolCallbacks } from "../../tools/host-provider.js";

// ---- Queue types ----

/** Queue consumption mode. */
export type QueueMode = "one-at-a-time" | "all";

export interface SteeringMessage {
  text: string;
  images?: ImageContent[];
}

export interface FollowUpMessage {
  text: string;
  images?: ImageContent[];
}

export interface NextTurnMessage {
  text: string;
  images?: ImageContent[];
}

// ---- Host options ----

export type ToolApprovalHandler = (request: ToolApprovalRequest) => Promise<"accept" | "decline">;

export interface PikoHostCreateOptions {
  /**
   * Model step executor used to create the default orchestrator.
   * PikoHost itself owns the orchestrator boundary, not the executor.
   */
  engine?: ModelStepExecutor;
  config: HostConfig;
  approvalHandler?: ToolApprovalHandler;
  /** Callbacks for model-initiated host tools such as ask_user/request_user_input. */
  hostToolCallbacks?: HostToolCallbacks;
  systemPrompt?: string;
  session?: CreateSessionRuntimeOptions;
  /** Append to system prompt (after default). */
  appendSystemPrompt?: string;
  /** Custom guidelines for the system prompt. */
  promptGuidelines?: string[];
  /** Prompt templates (loaded from .piko/prompts/). */
  promptTemplates?: PromptTemplate[];
  /** Settings manager for layered configuration (compaction, model defaults, etc.). */
  settingsManager?: SettingsManager;
  /** Model registry for model discovery and API key resolution. */
  modelRegistry?: ModelRegistry;
  /** Skip loading AGENTS.md / CLAUDE.md context files. */
  skipContextFiles?: boolean;
  /**
   * Optional Orchestrator for multi-agent team mode.
   * When provided, the host registers a default team and wires orchestrator events.
   */
  orchestrator?: Orchestrator;
}

export interface StreamPromptOptions {
  settingsOverride?: Partial<ModelRunSettings>;
  images?: ImageContent[];
  agentId?: string;
}

export interface StreamPromptResult {
  messages: Message[];
  appendedMessages: Message[];
  status: HostRunResult["status"];
  sessionId: string;
  sessionFile?: string;
}

export interface HostRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "context_overflow";
  sessionId: string;
  sessionFile?: string;
}
