// ============================================================================
// shared/types — TUI view-model types (camelCase, UI-oriented)
//
// LAYER: View models used by state, reducers, selectors, and renderer.
//
// These are NOT wire types. Wire types live in client/hostd-protocol.ts
// and use snake_case to match the Rust protocol. The mapping between wire
// types and view-model types happens in client/hostd-events.ts.
//
// RULE: If a type appears in a HostEvent or HostCommand, it belongs in
// hostd-protocol.ts. If it's used for TUI rendering/state, it belongs here.
// ============================================================================

// ============================================================================
// Message content types
// ============================================================================

export interface TextContent {
  type: "text";
  text: string;
  textSignature?: string;
}

export interface ThinkingContent {
  type: "thinking";
  thinking: string;
  thinkingSignature?: string;
  redacted?: boolean;
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
  thoughtSignature?: string;
}

// ============================================================================
// Usage
// ============================================================================

export interface Usage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  totalTokens: number;
  cost: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
    total: number;
  };
}

// ============================================================================
// Message types
// ============================================================================

export type Api = string;

export interface UserMessage {
  role: "user";
  content: string | (TextContent | ImageContent)[];
  timestamp: number;
}

export interface AssistantMessage {
  role: "assistant";
  content: (TextContent | ThinkingContent | ToolCall)[];
  api: Api;
  provider: string;
  model: string;
  responseModel?: string;
  responseId?: string;
  diagnostics?: AssistantMessageDiagnostic[];
  usage: Usage;
  stopReason: StopReason;
  errorMessage?: string;
  timestamp: number;
}

export interface DiagnosticErrorInfo {
  name?: string;
  message: string;
  stack?: string;
  code?: string | number;
}

export interface AssistantMessageDiagnostic {
  type: string;
  timestamp: number;
  error?: DiagnosticErrorInfo;
  details?: Record<string, unknown>;
}

export type StopReason = "stop" | "length" | "toolUse" | "error" | "aborted";

export interface ToolResultMessage<TDetails = unknown> {
  role: "toolResult";
  toolCallId: string;
  toolName: string;
  content: (TextContent | ImageContent)[];
  details?: TDetails;
  isError: boolean;
  timestamp: number;
}

export type Message = UserMessage | AssistantMessage | ToolResultMessage;

// ============================================================================
// Model types
// ============================================================================

export type ModelInputKind = "text" | "image";

export interface Model<TApi extends Api = Api> {
  id: string;
  name: string;
  api: TApi;
  provider: string;
  baseUrl: string;
  reasoning: boolean;
  thinkingLevelMap?: Partial<Record<string, string | null>>;
  input: ModelInputKind[];
  cost: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
  };
  contextWindow: number;
  maxTokens: number;
  headers?: Record<string, string>;
  compat?: unknown;
}

export interface ModelSummary {
  id: string;
  name: string;
  reasoning: boolean;
  input: ModelInputKind[];
  contextWindow: number;
  maxTokens: number;
}

export interface ProviderInfo {
  provider: string;
  models: ModelSummary[];
}

export interface ToolInfo {
  name: string;
  description: string;
}

export interface ModelCapabilities {
  supportsTools: boolean;
  supportsSandbox: boolean;
  supportsMCP: boolean;
  tools: ToolInfo[];
}

export interface ModelProviderConfig {
  apiKey?: string;
  headers?: Record<string, string>;
  reasoning?: { effort?: string; summary?: string };
  sessionId?: string;
  baseUrl?: string;
  extra?: Record<string, unknown>;
}

export interface ModelRuntimeLimits {
  maxModelCalls?: number;
  maxToolCalls?: number;
  maxWallClockMs?: number;
  maxConsecutiveErrors?: number;
  perToolTimeoutMs?: number;
}

export interface ModelRuntimeCounters {
  modelCalls: number;
  toolCalls: number;
  consecutiveErrors: number;
  startedAt: number;
}

export interface ModelRunSettings {
  parallelTools?: boolean;
  allowToolCalls: boolean;
  thinkingLevel?: string;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: { stopOnAssistantMessage?: boolean; stopOnToolResult?: boolean };
  runtimeLimits?: ModelRuntimeLimits;
  maxTokens?: number;
}

export interface ResolvedModel {
  provider: string;
  model: ModelSummary;
  providerConfig: ModelProviderConfig;
}

// ============================================================================
// Approval types
// ============================================================================

export type ToolApprovalDecision =
  | "accept"
  | "decline"
  | "accept_session"
  | "accept_workspace"
  | "accept_permanent";

export interface ToolApprovalRequest {
  toolEntityId: string;
  callId: string;
  agentId: string;
  taskId: string;
  toolName: string;
  toolArgs: Record<string, unknown>;
}

// ============================================================================
// Runtime stream types (TUI rendering)
// ============================================================================

export interface RuntimeTextBlock {
  type: "text";
  text: string;
}

export interface RuntimeThinkingBlock {
  type: "thinking";
  thinking: string;
  thinkingSignature?: string;
}

export interface RuntimeToolCallBlock {
  type: "toolCall";
  id: string;
  name: string;
  arguments: unknown;
  partialJson?: string;
}

export type RuntimeUserContentBlock = RuntimeTextBlock | ImageContent;

export type RuntimeAssistantContentBlock =
  | RuntimeTextBlock
  | RuntimeThinkingBlock
  | RuntimeToolCallBlock;

export type RuntimeMessageRole = "user" | "assistant" | "toolResult" | "custom";

export interface RuntimeMessageBase {
  id: string;
  role: RuntimeMessageRole;
  timestamp?: number;
}

export interface RuntimeUserMessage extends RuntimeMessageBase {
  role: "user";
  content: RuntimeUserContentBlock[];
}

export interface RuntimeAssistantMessage extends RuntimeMessageBase {
  role: "assistant";
  content: RuntimeAssistantContentBlock[];
  isStreaming?: boolean;
  stopReason?: string;
  errorMessage?: string;
  usage?: Usage;
  provider?: string;
  model?: string;
}

export interface RuntimeToolResultMessage extends RuntimeMessageBase {
  role: "toolResult";
  toolCallId: string;
  toolName?: string;
  content: unknown;
  isError?: boolean;
}

export interface RuntimeCustomMessage extends RuntimeMessageBase {
  role: "custom";
  customType: string;
  content: unknown;
}

export type RuntimeMessage =
  | RuntimeUserMessage
  | RuntimeAssistantMessage
  | RuntimeToolResultMessage
  | RuntimeCustomMessage;

export type RuntimeAssistantMessageEvent =
  | { type: "start" }
  | { type: "text_start"; contentIndex: number }
  | { type: "text_delta"; contentIndex: number; delta: string }
  | { type: "text_end"; contentIndex: number }
  | { type: "thinking_start"; contentIndex: number }
  | { type: "thinking_delta"; contentIndex: number; delta: string }
  | { type: "thinking_end"; contentIndex: number; contentSignature?: string }
  | { type: "toolcall_start"; contentIndex: number; id: string; name: string }
  | { type: "toolcall_delta"; contentIndex: number; delta: string }
  | { type: "toolcall_end"; contentIndex: number }
  | { type: "done" }
  | { type: "error"; message: string };

// ============================================================================
// TextSegment — rich text segment for SelectListView and tree display
// ============================================================================

export interface TextSegment {
  text: string;
  /** Theme token path, e.g. "text.accent", "text.muted", "border.accent" */
  color?: string;
}

// ============================================================================
// Session tree entry types — mirror of hostd wire format
// ============================================================================

export interface SessionTreeEntryBase {
  type: string;
  id: string;
  parentId: string | null;
  timestamp: string;
}

export type BashExecutionMessage = {
  role: "bashExecution";
  content?: string | (TextContent | ImageContent)[];
  [key: string]: unknown;
};

export type CustomPersistedMessage = {
  role: "custom";
  content?: string | (TextContent | ImageContent)[];
  customType?: string;
  [key: string]: unknown;
};

export type PersistableMessage = Message | BashExecutionMessage | CustomPersistedMessage;

export interface MessageEntry extends SessionTreeEntryBase {
  type: "message";
  message: PersistableMessage;
}

export interface ThinkingLevelChangeEntry extends SessionTreeEntryBase {
  type: "thinking_level_change";
  thinkingLevel: string;
}

export interface ModelChangeEntry extends SessionTreeEntryBase {
  type: "model_change";
  provider: string;
  modelId: string;
}

export interface ActiveToolsChangeEntry extends SessionTreeEntryBase {
  type: "active_tools_change";
  activeToolNames: string[];
}

export interface CompactionEntry<T = unknown> extends SessionTreeEntryBase {
  type: "compaction";
  summary: string;
  firstKeptEntryId: string;
  tokensBefore: number;
  details?: T;
  fromHook?: boolean;
}

export interface BranchSummaryEntry<T = unknown> extends SessionTreeEntryBase {
  type: "branch_summary";
  fromId: string;
  summary: string;
  details?: T;
  fromHook?: boolean;
}

export interface CustomEntry<T = unknown> extends SessionTreeEntryBase {
  type: "custom";
  customType: string;
  data?: T;
}

export interface CustomMessageEntry<T = unknown> extends SessionTreeEntryBase {
  type: "custom_message";
  customType: string;
  content: string | (TextContent | ImageContent)[];
  details?: T;
  display: boolean;
}

export interface LabelEntry extends SessionTreeEntryBase {
  type: "label";
  targetId: string;
  label: string | undefined;
}

export interface SessionInfoEntry extends SessionTreeEntryBase {
  type: "session_info";
  name?: string;
}

export interface LeafEntry extends SessionTreeEntryBase {
  type: "leaf";
  targetId: string | null;
}

export type SessionTreeEntry =
  | MessageEntry
  | ThinkingLevelChangeEntry
  | ModelChangeEntry
  | ActiveToolsChangeEntry
  | CompactionEntry
  | BranchSummaryEntry
  | CustomEntry
  | CustomMessageEntry
  | LabelEntry
  | SessionInfoEntry
  | LeafEntry;

/** Tree node for session tree display in TUI. */
export interface SessionTreeNode {
  entry: SessionTreeEntry;
  children: SessionTreeNode[];
  label?: string;
  labelTimestamp?: string;
}

/** Result of navigating a session tree to a specific entry. */
export type TreeNavigationResult = {
  status: "navigated" | "already_current";
  sessionId: string;
  oldLeafId: string | null;
  newLeafId: string | null;
  selectedEntryId: string;
  branchEntries: SessionTreeEntry[];
  editorContent?: unknown;
};

// ============================================================================
// OrchState — kept as a minimal stub since hostd always returns undefined.
// Multi-agent panel data will come through hostd protocol in the future.
// ============================================================================

export interface OrchAgentState {
  id: string;
  status: "idle" | "running" | "failed" | "stopped";
  spec?: { name?: string };
  activeTaskId?: string | null;
}

export interface OrchTaskState {
  id: string;
  prompt: string;
  plan?: unknown[];
  error?: string;
  status?: string;
}

export interface OrchState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, unknown>;
  agents: Record<string, OrchAgentState>;
  tasks: Record<string, OrchTaskState>;
}
