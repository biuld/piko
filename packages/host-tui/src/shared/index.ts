// ============================================================================
// @piko/shared — protocol types plus pure utilities for host-tui.
//
// Host runtime state such as auth, model discovery, settings storage, tools,
// compaction, sessions, and filesystem access is owned by hostd.
// This module retains only protocol type mirrors and pure display utilities.
// ============================================================================

// ---- Debug tracing ----

export { installDebugTraceFromEnv } from "./debug/file-trace.js";
export * from "./debug/trace.js";

// ---- Protocol types (thin-client subset) ----

export type {
  AssistantMessage,
  AssistantMessageDiagnostic,
  DiagnosticErrorInfo,
  ImageContent,
  Message,
  Model,
  ModelCapabilities,
  ModelProviderConfig,
  ModelRunSettings,
  ModelRuntimeCounters,
  ModelRuntimeLimits,
  ModelSummary,
  OrchAgentState,
  OrchState,
  OrchTaskState,
  ProviderInfo,
  ResolvedModel,
  RuntimeAssistantContentBlock,
  RuntimeAssistantMessage,
  RuntimeAssistantMessageEvent,
  RuntimeCustomMessage,
  RuntimeMessage,
  RuntimeMessageRole,
  RuntimeTextBlock,
  RuntimeThinkingBlock,
  RuntimeToolCallBlock,
  RuntimeToolResultMessage,
  RuntimeUserContentBlock,
  RuntimeUserMessage,
  StopReason,
  TextContent,
  ThinkingContent,
  ToolApprovalDecision,
  ToolApprovalRequest,
  ToolCall,
  ToolInfo,
  ToolResultMessage,
  Usage,
  UserMessage,
} from "./types.js";

// ---- Context files (inline type) ----

export interface ContextFile {
  path: string;
  content: string;
}

// ---- Session display utilities (pure functions only) ----

export type {
  FlatTreeEntry,
  FlattenedTreeItem,
  GutterInfo,
  SessionHandle,
  SessionMeta,
  SessionTreeEntry,
  SessionTreeNode,
  TextSegment,
  TreeNavigationResult,
} from "./session/index.js";
export {
  buildSessionTree,
  flattenSessionTree,
  getEntryLabel,
  getEntrySegments,
  getSearchableText,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "./session/index.js";

// ---- Utility types & functions (pure, no host-side concerns) ----

export type { CumulativeUsage } from "./utils/index.js";
export {
  basenamePath,
  computeCumulativeUsage,
  dirnamePath,
  extnamePath,
  isAbsolutePath,
  joinPath,
  parsePath,
  pathSeparator,
  resolvePath,
} from "./utils/index.js";
