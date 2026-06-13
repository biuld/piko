import type {
  ImageContent,
  Message,
  ModelRunSettings,
  ModelStepExecutor,
  Orchestrator,
  ToolApprovalRequest,
} from "piko-orchestrator";
import type { HostConfig } from "../models/index.js";
import type { PromptTemplate } from "../prompts/index.js";
import type { CreateSessionRuntimeOptions } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";

export type { HostToolCallbacks } from "../tools/host-provider.js";

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
  /** Model step executor. Defaults to native executor with pi-ai LLM caller. */
  engine?: ModelStepExecutor;
  config: HostConfig;
  approvalHandler?: ToolApprovalHandler;
  /** Callbacks for model-initiated host tools such as ask_user/request_user_input. */
  hostToolCallbacks?: import("../tools/host-provider.js").HostToolCallbacks;
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
  /** Skip loading AGENTS.md / CLAUDE.md context files. */
  skipContextFiles?: boolean;
  /** Custom tools registered by extensions. */
  customTools?: Array<{
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    executor: (args: Record<string, unknown>) => Promise<unknown> | unknown;
  }>;
  /**
   * Optional Orchestrator for multi-agent team mode.
   * When provided, the host registers a default team and wires orchestrator events.
   */
  orchestrator?: Orchestrator;
}

export interface StreamPromptOptions {
  settingsOverride?: Partial<ModelRunSettings>;
  images?: ImageContent[];
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
  status: "completed" | "aborted" | "error" | "max_steps" | "context_overflow";
  sessionId: string;
  sessionFile?: string;
}
