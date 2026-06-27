// ============================================================================
// Session tree utilities (pure functions)
// ============================================================================

export {
  encodeCwd,
  ensurePikoDir,
  getAgentDir,
  getPikoDir,
  getSessionDir,
  getSessionsDir,
} from "./session-paths.js";
export type {
  FlatTreeEntry,
  FlattenedTreeItem,
  GutterInfo,
  TextSegment,
} from "./session-tree-utils/index.js";
export {
  buildSessionTree,
  flattenSessionTree,
  getEntryLabel,
  getEntrySegments,
  getSearchableText,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "./session-tree-utils/index.js";
// Re-export TreeNavigationResult for session actions
export type {
  CompactionEntry,
  FileEntry,
  MessageEntry,
  ModelChangeEntry,
  SessionHandle,
  SessionHeader,
  SessionInfoEntry,
  SessionMeta,
  SessionPersistenceOverview,
  SessionTreeEntry,
  SessionTreeNode,
  TreeNavigationResult,
  WriteSessionSnapshotOptions,
} from "./session-types.js";
