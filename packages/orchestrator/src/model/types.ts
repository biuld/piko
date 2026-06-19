// ---- ModelStepExecutor types — internal orchestrator subsystem ----
// Public config/capability types are in piko-orchestrator-protocol.
// This file retains the internal ModelStepExecutor subsystem types.

import type {
  EventStream,
  Message,
  ModelCapabilities,
  ModelProviderConfig,
  ModelRunSettings,
  ModelRuntimeCounters,
  RuntimeAssistantMessageEvent,
  RuntimeMessage,
  ToolDef,
  Usage,
} from "piko-orchestrator-protocol";

// ---- Continuation state ----

export type ModelContinuationState = ReadyContinuationState;

export interface ReadyContinuationState {
  version: 1;
  kind: "ready";
  counters?: ModelRuntimeCounters;
}

export interface ModelStepInput {
  runId: string;
  stepId: string;
  transcript: Message[];
  systemPrompt: string;
  model: import("piko-orchestrator-protocol").Model<string>;
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
  | { type: "message_end"; message: Message | RuntimeMessage }
  | { type: "step_end" }
  | { type: "error"; message: string }
  | { type: "provider_tool_call_delta"; id: string; name: string; argsDelta?: string }
  | { type: "message_start"; message: RuntimeMessage }
  | {
      type: "message_update";
      message: RuntimeMessage;
      assistantEvent?: RuntimeAssistantMessageEvent;
    };

// ---- Transcript delta ----

/** Durable facts appended to the transcript at step end. */
export type TranscriptDelta =
  | { kind: "assistant_message"; message: Message }
  | { kind: "tool_result"; message: Message; toolCallId: string };

// ---- Model step result ----
export type ModelStepStatus = "continue" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "abort" | "error";

interface ModelStepResultBase {
  appendedMessages: Message[];
  usage?: Usage;
  engineState?: unknown;
  stopReason?: StopReason;
  transcriptDelta?: TranscriptDelta[];
}

export type ModelStepResult =
  | (ModelStepResultBase & {
      status: "continue";
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

// ---- Core compute model ----

/**
 * Core compute model.
 *
 *   ModelStepCompute: ModelStepInput -> EventStream<ModelStepEvent, ModelStepResult>
 *
 * Tool execution and approval are handled entirely by `ToolRegistryImpl.executeTool()` (called from the AgentActor worker).
 * The model executor only calls the LLM and returns the result.
 */
export type ModelStepCompute = (
  input: ModelStepInput,
  signal?: AbortSignal,
) => EventStream<ModelStepEvent, ModelStepResult>;

// ---- ModelStepExecutor interface ----
export interface ModelStepExecutor {
  readonly capabilities: ModelCapabilities;
  executeStep: ModelStepCompute;
  shutdown?(): Promise<void>;
}

// ---- Remote ----
export interface ModelEventEnvelope {
  runId: string;
  stepId: string;
  event: ModelStepEvent;
}
