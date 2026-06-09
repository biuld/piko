import type {
  AgentOrchestrator,
  EngineRunSettings,
  ImageContent,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { HostConfig } from "../models/index.js";
import type { PromptTemplate } from "../prompts/index.js";
import type { CreateSessionRuntimeOptions } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";
import type { HostLifecycleEvent } from "./lifecycle-events.js";

export interface PikoHostCreateOptions {
  /** Engine implementation. Defaults to native engine with pi-ai LLM caller. */
  engine?: StatelessEngine;
  config: HostConfig;
  approvalHandler?: ApprovalHandler;
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
   * Optional AgentOrchestrator for multi-agent team mode.
   * When provided, the host registers a default team and wires orchestrator events.
   */
  orchestrator?: AgentOrchestrator;
}

export interface StreamPromptOptions {
  settingsOverride?: Partial<EngineRunSettings>;
  images?: ImageContent[];
  /** Callback for host-level lifecycle events (agent_start, turn_*, settled, etc.). */
  onLifecycleEvent?: (event: HostLifecycleEvent) => void;
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
