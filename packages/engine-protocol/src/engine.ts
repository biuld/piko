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
export type EngineEvent =
  | { type: "step_start" }
  | { type: "message_delta"; messageId: string; delta: string }
  | { type: "thinking_delta"; messageId: string; delta: string }
  | { type: "message_end"; message: Message }
  | { type: "tool_call_start"; id: string; name: string; args: Record<string, unknown> }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | { type: "approval_requested"; request: PendingApprovalState }
  | { type: "step_end" }
  | { type: "error"; message: string };

// ---- Engine step result ----
export type EngineStepStatus = "continue" | "awaiting_approval" | "completed" | "aborted" | "error";
export type StopReason = "assistant" | "tool" | "max_steps" | "approval" | "abort" | "error";
export interface EngineStepResult {
  status: EngineStepStatus;
  appendedMessages: Message[];
  usage?: TokenUsage;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
  stopReason?: StopReason;
}

// ---- Approval resolution ----
export interface EngineApprovalResolution {
  runId: string;
  stepId: string;
  approvalRequestId: string;
  decision: "accept" | "decline" | "acceptForSession";
  transcript: Message[];
  engineState?: unknown;
}

// ---- Engine interface ----
export interface StatelessEngine {
  readonly capabilities: EngineCapabilities;
  executeStep(input: EngineInput, signal?: AbortSignal): EventStream<EngineEvent, EngineStepResult>;
  resolveApproval?(
    request: EngineApprovalResolution,
    signal?: AbortSignal,
  ): Promise<EngineStepResult>;
  shutdown?(): Promise<void>;
}

// ---- Remote ----
export interface EngineEventEnvelope {
  runId: string;
  stepId: string;
  event: EngineEvent;
}
