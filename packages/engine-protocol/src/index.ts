// ---- Content types ----

export interface TextContent {
  type: "text";
  text: string;
}

export interface ImageContent {
  type: "image";
  data: string;
  mimeType: string;
}

export interface ToolCall {
  type: "toolCall";
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

// ---- Message types ----

export interface UserMessage {
  role: "user";
  content: string | (TextContent | ImageContent)[];
  timestamp: number;
}

export interface AssistantMessage {
  role: "assistant";
  content: (TextContent | ToolCall)[];
  timestamp: number;
}

export interface ToolResultMessage {
  role: "toolResult";
  toolCallId: string;
  toolName: string;
  content: (TextContent | ImageContent)[];
  isError: boolean;
  timestamp: number;
}

export type Message = UserMessage | AssistantMessage | ToolResultMessage;

// ---- Model ----

export interface EngineModel {
  id: string;
  name: string;
  api: string;
  provider: string;
  baseUrl: string;
  reasoning: boolean;
  input: ("text" | "image")[];
  contextWindow: number;
  maxTokens: number;
}

// ---- Token usage ----

export interface TokenUsage {
  input: number;
  output: number;
  total: number;
}

// ---- Engine capabilities ----

export interface EngineCapabilities {
  supportsApprovals: boolean;
  supportsTools: boolean;
  supportsSandbox: boolean;
  supportsMCP: boolean;
  maxSteps: number;
}

// ---- Engine input ----

export interface EngineProviderConfig {
  apiKey?: string;
  headers?: Record<string, string>;
  reasoning?: {
    effort?: string;
    summary?: string;
  };
  sessionId?: string;
  baseUrl?: string;
  extra?: Record<string, unknown>;
}

export interface EngineRunSettings {
  maxSteps: number;
  parallelTools: boolean;
  allowToolCalls: boolean;
  allowApprovals: boolean;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: {
    stopOnAssistantMessage?: boolean;
    stopOnToolResult?: boolean;
  };
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
  model: EngineModel;
  provider: EngineProviderConfig;
  tools: EngineTool[];
  settings: EngineRunSettings;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}

// ---- Engine events ----

export type EngineEvent =
  | { type: "step_start" }
  | { type: "provider_request"; request: unknown }
  | { type: "provider_response"; status?: number; headers?: Record<string, string> }
  | { type: "message_start"; message: Message }
  | { type: "message_delta"; messageId: string; delta: unknown }
  | { type: "message_end"; message: Message }
  | { type: "tool_call_start"; id: string; name: string; args: unknown }
  | { type: "tool_call_update"; id: string; partialResult: unknown }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | { type: "approval_requested"; request: PendingApprovalState }
  | { type: "state_snapshot"; engineState: unknown }
  | { type: "step_end" }
  | { type: "error"; message: string };

// ---- Engine step result ----

export type EngineStepStatus =
  | "continue"
  | "awaiting_approval"
  | "completed"
  | "aborted"
  | "error";

export type StopReason =
  | "assistant"
  | "tool"
  | "max_steps"
  | "approval"
  | "abort"
  | "error";

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

// ---- EventStream ----

export class EventStream<T, R = T> implements AsyncIterable<T> {
  private queue: T[] = [];
  private waiting: ((value: IteratorResult<T>) => void)[] = [];
  private done = false;
  private finalResultPromise: Promise<R>;
  private resolveFinalResult!: (result: R) => void;

  constructor() {
    this.finalResultPromise = new Promise((resolve) => {
      this.resolveFinalResult = resolve;
    });
  }

  push(event: T): void {
    if (this.done) return;
    const waiter = this.waiting.shift();
    if (waiter) {
      waiter({ value: event, done: false });
    } else {
      this.queue.push(event);
    }
  }

  end(result: R): void {
    this.done = true;
    this.resolveFinalResult(result);
    while (this.waiting.length > 0) {
      const waiter = this.waiting.shift()!;
      waiter({ value: undefined as unknown as T, done: true });
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    while (true) {
      if (this.queue.length > 0) {
        yield this.queue.shift()!;
      } else if (this.done) {
        return;
      } else {
        const result = await new Promise<IteratorResult<T>>(
          (resolve) => this.waiting.push(resolve),
        );
        if (result.done) return;
        yield result.value;
      }
    }
  }

  result(): Promise<R> {
    return this.finalResultPromise;
  }
}

// ---- Engine interface ----

export interface StatelessEngine {
  readonly capabilities: EngineCapabilities;

  executeStep(
    input: EngineInput,
    signal?: AbortSignal,
  ): EventStream<EngineEvent, EngineStepResult>;

  resolveApproval?(
    request: EngineApprovalResolution,
    signal?: AbortSignal,
  ): Promise<EngineStepResult>;

  shutdown?(): Promise<void>;
}

// ---- Remote engine protocol (JSON-RPC envelope) ----

export interface EngineEventEnvelope {
  runId: string;
  stepId: string;
  event: EngineEvent;
}
