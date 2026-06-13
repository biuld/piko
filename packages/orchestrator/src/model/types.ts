// ---- ModelStepExecutor types — internal orchestrator subsystem ----
// Public config/capability types are in piko-orchestrator-protocol.
// This file retains the internal ModelStepExecutor subsystem types.

import type {
  ModelCapabilities,
  ModelProviderConfig,
  ModelRunSettings,
  ModelRuntimeCounters,
  ToolDef,
} from "piko-orchestrator-protocol";
import type { EventStream, Message, Usage } from "./event-stream.js";

// ---- Continuation state ----
export type ModelContinuationState = ReadyContinuationState | PendingToolsContinuationState;

export interface ModelResumeContext {
  systemPrompt: string;
  model: import("./event-stream.js").Model<string>;
  provider: ModelProviderConfig;
  tools?: ToolDef[];
  settings: ModelRunSettings;
}

export interface ReadyContinuationState {
  version: 1;
  kind: "ready";
  pendingToolCalls?: undefined;
  counters?: ModelRuntimeCounters;
}

export interface PendingToolsContinuationState {
  version: 1;
  kind: "pending_tools";
  pendingToolCalls: PendingToolCallState;
  resumeContext: ModelResumeContext;
  counters?: ModelRuntimeCounters;
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
  settings: ModelRunSettings;
}

export interface ModelStepInput {
  runId: string;
  stepId: string;
  transcript: Message[];
  systemPrompt: string;
  model: import("./event-stream.js").Model<string>;
  provider: ModelProviderConfig;
  /** Tools visible for this model step. */
  tools?: ToolDef[];
  settings: ModelRunSettings;
  engineState?: unknown;
}

// ---- Model step events ----

/** Unified model step event type. */
export type ModelStepEvent =
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

// ---- Model step result ----
export type ModelStepStatus = "continue" | "awaiting_resource" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "tool" | "max_steps" | "resource" | "abort" | "error";

interface ModelStepResultBase {
  appendedMessages: Message[];
  usage?: Usage;
  engineState?: unknown;
  stopReason?: StopReason;
  /** Durable transcript delta: the canonical persistence API. */
  transcriptDelta?: TranscriptDelta[];
}

export type ModelStepResult =
  | (ModelStepResultBase & {
      status: "continue";
    })
  | (ModelStepResultBase & {
      status: "awaiting_resource";
      pendingTools: PendingToolCallState;
      stopReason: "resource";
    })
  | (ModelStepResultBase & {
      status: "completed";
    })
  | (ModelStepResultBase & {
      status: "aborted";
      stopReason: "abort";
    })
  | (ModelStepResultBase & {
      status: "error";
      stopReason: "error";
    });

// ---- Resource resolution (tool execution) ----
export interface ModelResourceResolution {
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
 *   ModelStepCompute: ModelStepInput -> EventStream<ModelStepEvent, ModelStepResult>
 *
 * Tool execution and approval are handled by ToolActor/AgentActor.
 * The model executor returns an event stream; the caller feeds tool results back
 * via resolveResource() if the step yields pending tools.
 */
export type ModelStepCompute = (
  input: ModelStepInput,
  signal?: AbortSignal,
) => EventStream<ModelStepEvent, ModelStepResult>;

// ---- ModelStepExecutor interface ----
export interface ModelStepExecutor {
  readonly capabilities: ModelCapabilities;
  executeStep: ModelStepCompute;
  /** Resolve an awaiting_resource step with tool results. */
  resolveResource?: (
    resolution: ModelResourceResolution,
    signal?: AbortSignal,
  ) => Promise<ModelStepResult>;
  shutdown?(): Promise<void>;
}

// ---- Remote ----
export interface ModelEventEnvelope {
  runId: string;
  stepId: string;
  event: ModelStepEvent;
}
