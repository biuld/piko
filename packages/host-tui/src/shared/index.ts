// ============================================================================
// @piko/shared — protocol types plus pure utilities for host-tui.
//
// Host runtime state such as auth, model discovery, settings storage, tools,
// compaction, sessions, and filesystem access is owned by hostd.
// This module retains only protocol type mirrors and pure display utilities.
// ============================================================================

// ---- Debug tracing ----

export { debugTrace, startDebugSpan } from "./trace.js";

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
  SessionTreeEntry,
  SessionTreeNode,
  StopReason,
  TextContent,
  TextSegment,
  ThinkingContent,
  ToolApprovalDecision,
  ToolApprovalRequest,
  ToolCall,
  ToolInfo,
  ToolResultMessage,
  TreeNavigationResult,
  Usage,
  UserMessage,
} from "./types.js";

// ---- Context files (inline type) ----

export interface ContextFile {
  path: string;
  content: string;
}

// ---- Session display utilities (pure functions on hostd-provided data) ----

export {
  getEntryLabel,
  getEntrySegments,
  getSearchableText,
} from "./session-display.js";
export type {
  FlatTreeEntry,
  FlattenedTreeItem,
  GutterInfo,
} from "./session-flat-tree.js";
export {
  flattenSessionTree,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "./session-flat-tree.js";
export { buildSessionTree } from "./session-tree.js";

// ---- Pure utility functions ----

export {
  basenamePath,
  dirnamePath,
  extnamePath,
  isAbsolutePath,
  joinPath,
  parsePath,
  pathSeparator,
  resolvePath,
} from "./bun-path.js";
export type { CumulativeUsage } from "./token-usage.js";
export { computeCumulativeUsage } from "./token-usage.js";
