export type ModelStepEvent =
  | { type: "step_start" }
  | { type: "message_delta"; messageId: string; delta: string }
  | { type: "thinking_delta"; messageId: string; delta: string }
  | { type: "message_end"; message: unknown }
  | { type: "step_end" }
  | { type: "error"; message: string };

export interface ModelStepResult {
  status: ModelStepStatus;
  appendedMessages: unknown[];
  usage?: unknown;
  engineState?: unknown;
  stopReason?: StopReason;
  transcriptDelta?: unknown[];
}

export type ModelStepStatus = "continue" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "abort" | "error";
export type TranscriptDelta =
  | { kind: "assistant_message"; message: unknown }
  | { kind: "tool_result"; message: unknown; toolCallId: string };
