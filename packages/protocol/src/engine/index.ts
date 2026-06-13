import type { ToolDef } from "../tools/index.js";
import type { EventStream, Message, TokenUsage } from "../types.js";

// ---- Engine capabilities ----
export interface ToolInfo {
  name: string;
  description: string;
}
export interface EngineCapabilities {
  supportsTools: boolean;
  supportsSandbox: boolean;
  supportsMCP: boolean;
  tools: ToolInfo[];
}

// ---- Engine input ----
export interface EngineProviderConfig {
  apiKey?: string;
  headers?: Record<string, string>;
  reasoning?: { effort?: string; summary?: string };
  sessionId?: string;
  baseUrl?: string;
  extra?: Record<string, unknown>;
}

// ---- Runtime limits ----
export interface EngineRuntimeLimits {
  maxModelCalls?: number;
  maxToolCalls?: number;
  maxWallClockMs?: number;
  maxConsecutiveErrors?: number;
  perToolTimeoutMs?: number;
}

export interface EngineRuntimeCounters {
  modelCalls: number;
  toolCalls: number;
  consecutiveErrors: number;
  startedAt: number;
}

export interface EngineRunSettings {
  maxSteps: number;
  parallelTools?: boolean;
  allowToolCalls: boolean;
  thinkingLevel?: string;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: { stopOnAssistantMessage?: boolean; stopOnToolResult?: boolean };
  runtimeLimits?: EngineRuntimeLimits;
}

// ---- Engine continuation state ----
export type EngineContinuationState = ReadyContinuationState | PendingToolsContinuationState;

export interface ReadyContinuationState {
  version: 1;
  kind: "ready";
  pendingToolCalls?: undefined;
  counters?: EngineRuntimeCounters;
}

export interface PendingToolsContinuationState {
  version: 1;
  kind: "pending_tools";
  pendingToolCalls: PendingToolCallState;
  counters?: EngineRuntimeCounters;
}

export interface PendingToolCallState {
  assistantMessage: Message;
  remainingToolCallIds: string[];
  toolCalls: Array<{
    id: string;
    name: string;
    args: Record<string, unknown>;
    /** Registry key used to look up the executor. Defaults to name when absent. */
    executorTarget?: string;
    executionMode?: "sequential" | "parallel";
    requiresApproval?: boolean;
  }>;
  /** Tool execution settings preserved across approval pauses. */
  settings: EngineRunSettings;
}

export interface EngineInput {
  runId: string;
  stepId: string;
  transcript: Message[];
  systemPrompt: string;
  model: import("../types.js").Model<string>;
  provider: EngineProviderConfig;
  /** ToolSets: grouped capability surfaces. When provided, tools are projected from these. */
  toolSets?: import("../tools/index.js").ToolSet[];
  /** Legacy flat tool list. Supported for backward compat. If toolSets is provided, tools is ignored. */
  tools?: ToolDef[];
  settings: EngineRunSettings;
  engineState?: unknown;
}

// ---- Engine events ----

/** Unified Engine event type. */
export type EngineEvent =
  | { type: "step_start" }
  | { type: "message_delta"; messageId: string; delta: string }
  | { type: "thinking_delta"; messageId: string; delta: string }
  | { type: "message_end"; message: Message }
  | { type: "resource_requested"; request: PendingToolCallState }
  | { type: "step_end" }
  | { type: "error"; message: string }
  | { type: "provider_tool_call_delta"; id: string; name: string; argsDelta?: string };

// ---- Transcript delta ----

/** Durable facts appended to the transcript at step end. */
export type TranscriptDelta =
  | { kind: "assistant_message"; message: Message }
  | { kind: "tool_result"; message: Message; toolCallId: string };

// ---- Engine step result ----
export type EngineStepStatus = "continue" | "awaiting_resource" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "tool" | "max_steps" | "resource" | "abort" | "error";

interface EngineStepResultBase {
  appendedMessages: Message[];
  usage?: TokenUsage;
  engineState?: unknown;
  stopReason?: StopReason;
  /** Durable transcript delta: the canonical persistence API. */
  transcriptDelta?: TranscriptDelta[];
}

export type EngineStepResult =
  | (EngineStepResultBase & {
      status: "continue";
    })
  | (EngineStepResultBase & {
      status: "awaiting_resource";
      pendingTools: PendingToolCallState;
      stopReason: "resource";
    })
  | (EngineStepResultBase & {
      status: "completed";
    })
  | (EngineStepResultBase & {
      status: "aborted";
      stopReason: "abort";
    })
  | (EngineStepResultBase & {
      status: "error";
      stopReason: "error";
    });

// ---- Resource resolution (tool execution) ----
export interface EngineResourceResolution {
  runId: string;
  stepId: string;
  transcript: Message[];
  engineState?: unknown;
  /** Tool results to feed back to the engine. */
  toolResults?: Array<{ toolCallId: string; result: unknown; isError: boolean }>;
}

// ---- Core compute model ----

/**
 * Core compute model.
 *
 *   EngineCompute: EngineInput -> EventStream<EngineEvent, EngineStepResult>
 *
 * Tool execution and approval are handled by ToolActor/AgentActor.
 * The engine returns an event stream; the caller feeds tool results back
 * via resolveResource() if the step yields pending tools.
 */
export type EngineCompute = (
  input: EngineInput,
  signal?: AbortSignal,
) => EventStream<EngineEvent, EngineStepResult>;

// ---- Engine interface ----
export interface StatelessEngine {
  readonly capabilities: EngineCapabilities;
  executeStep: EngineCompute;
  /** Resolve an awaiting_resource step with tool results. */
  resolveResource?: (
    resolution: EngineResourceResolution,
    signal?: AbortSignal,
  ) => Promise<EngineStepResult>;
  shutdown?(): Promise<void>;
}

// ---- Remote ----
export interface EngineEventEnvelope {
  runId: string;
  stepId: string;
  event: EngineEvent;
}
