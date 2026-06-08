import type { EventStream, Message, TokenUsage } from "./types.js";

// ---- Engine capabilities ----
export interface EngineToolInfo {
  name: string;
  description: string;
}
export interface EngineCapabilities {
  supportsApprovals: boolean;
  supportsTools: boolean;
  supportsSandbox: boolean;
  supportsMCP: boolean;
  maxSteps: number;
  tools: EngineToolInfo[];
  /** Full tool definitions (with inputSchema and executor). Used by host for active tools filtering. */
  engineTools?: EngineTool[];
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
  maxApprovalRequests?: number;
  perToolTimeoutMs?: number;
}

export interface EngineRuntimeCounters {
  modelCalls: number;
  toolCalls: number;
  approvalRequests: number;
  consecutiveErrors: number;
  startedAt: number;
}

export interface EngineRunSettings {
  maxSteps: number;
  /** Defaults to true when omitted. Set false to force sequential tool execution. */
  parallelTools?: boolean;
  allowToolCalls: boolean;
  allowApprovals: boolean;
  /** Thinking/reasoning level ("off" | "minimal" | "low" | "medium" | "high" | "xhigh") */
  thinkingLevel?: string;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: { stopOnAssistantMessage?: boolean; stopOnToolResult?: boolean };
  /** Runtime limits enforced by the Engine. */
  runtimeLimits?: EngineRuntimeLimits;
}
export interface EngineTool {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: EngineToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  metadata?: Record<string, unknown>;
}
export interface EngineToolExecutorRef {
  kind: "native" | "remote" | "sandbox" | "mcp";
  target: string;
  extra?: Record<string, unknown>;
}
export interface PendingApprovalState {
  requestId: string;
  kind: string;
  details: unknown;
  engineState?: unknown;
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
  settings: Pick<EngineRunSettings, "parallelTools" | "runtimeLimits"> & {
    allowApprovals?: boolean;
  };
}

export interface EngineInput {
  runId: string;
  stepId: string;
  transcript: Message[];
  systemPrompt: string;
  model: import("./types.js").Model<string>;
  provider: EngineProviderConfig;
  tools?: EngineTool[];
  settings: EngineRunSettings;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}

// ---- Engine events ----

/** Normalized provider events emitted during streaming. */
export type EngineProviderEvent =
  | { type: "provider_request_start"; provider: string; model: string }
  | { type: "provider_response_start"; status?: number; headers?: Record<string, string> }
  | { type: "provider_text_delta"; messageId: string; delta: string }
  | { type: "provider_thinking_delta"; messageId: string; delta: string }
  | { type: "provider_tool_call_delta"; id: string; name?: string; argsDelta?: string }
  | { type: "provider_message_end"; message: Message; usage?: TokenUsage }
  | { type: "provider_error"; message: string; retryable: boolean };

/** Normalized tool lifecycle events emitted during tool execution. */
export type EngineToolEvent =
  | { type: "tool_validation_start"; id: string; name: string }
  | { type: "tool_validation_end"; id: string; ok: boolean; error?: string }
  | { type: "tool_call_start"; id: string; name: string; args: Record<string, unknown> }
  | { type: "tool_stdout"; id: string; delta: string }
  | { type: "tool_stderr"; id: string; delta: string }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | {
      type: "tool_call_skipped";
      id: string;
      reason: "approval_required" | "disabled" | "limit" | "invalid";
    };

/** Unified Engine event type. Includes both rendering events and normalized lifecycle events. */
export type EngineEvent =
  | { type: "step_start" }
  | { type: "message_delta"; messageId: string; delta: string }
  | { type: "thinking_delta"; messageId: string; delta: string }
  | { type: "message_end"; message: Message }
  | { type: "tool_call_start"; id: string; name: string; args: Record<string, unknown> }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | { type: "approval_requested"; request: PendingApprovalState }
  | { type: "step_end" }
  | { type: "error"; message: string }
  // Normalized provider events (emitted during provider streaming)
  | EngineProviderEvent
  // Normalized tool lifecycle events (emitted during tool execution)
  | EngineToolEvent;

// ---- Transcript delta ----

/** Durable facts appended to the transcript at step end. */
export type TranscriptDelta =
  | { kind: "assistant_message"; message: Message }
  | { kind: "tool_result"; message: Message; toolCallId: string }
  | {
      kind: "approval_record";
      requestId: string;
      decision: "accept" | "decline" | "acceptForSession";
    };

// ---- Engine step result ----
export type EngineStepStatus = "continue" | "awaiting_approval" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "tool" | "max_steps" | "approval" | "abort" | "error";

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
      pendingApproval?: undefined;
    })
  | (EngineStepResultBase & {
      status: "awaiting_approval";
      pendingApproval: PendingApprovalState;
      stopReason: "approval";
    })
  | (EngineStepResultBase & {
      status: "completed";
      pendingApproval?: undefined;
    })
  | (EngineStepResultBase & {
      status: "aborted";
      pendingApproval?: undefined;
      stopReason: "abort";
    })
  | (EngineStepResultBase & {
      status: "error";
      pendingApproval?: undefined;
      stopReason: "error";
    });

// ---- Approval resolution ----
export interface EngineApprovalResolution {
  runId: string;
  stepId: string;
  approvalRequestId: string;
  decision: "accept" | "decline" | "acceptForSession";
  transcript: Message[];
  engineState?: unknown;
}

// ---- Core compute model ----

/**
 * Core Engine definition.
 *
 * Mathematically:
 *
 *   EngineCompute:
 *     EngineInput -> EventStream<EngineEvent, EngineStepResult>
 *
 * Expanded:
 *
 *   Snapshot × Runtime × ToolCatalog × ApprovalState
 *     -> Stream<Event> × StepResult
 *
 * The transcript is owned by the Host. The Engine computes over the snapshot
 * and returns durable deltas plus explicit continuation state; it must not rely
 * on hidden in-memory state to resume a step.
 */
export type EngineCompute = (
  input: EngineInput,
  signal?: AbortSignal,
) => EventStream<EngineEvent, EngineStepResult>;

/**
 * Continuation function for an approval pause.
 *
 * Mathematically:
 *
 *   ApprovalContinuation:
 *     EngineApprovalResolution -> EngineStepResult
 *
 * Any data needed to resume must be present in the resolution transcript and
 * typed engineState produced by the previous EngineCompute result.
 */
export type EngineApprovalContinuation = (
  request: EngineApprovalResolution,
  signal?: AbortSignal,
) => Promise<EngineStepResult>;

// ---- Engine interface ----
export interface StatelessEngine {
  readonly capabilities: EngineCapabilities;
  executeStep: EngineCompute;
  resolveApproval?: EngineApprovalContinuation;
  shutdown?(): Promise<void>;
}

// ---- Remote ----
export interface EngineEventEnvelope {
  runId: string;
  stepId: string;
  event: EngineEvent;
}
