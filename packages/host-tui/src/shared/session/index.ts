// ============================================================================
// Session display utilities (pure functions, no filesystem access)
//
// Session storage, I/O, and tree construction are owned by hostd.
// TUI only retains type definitions and pure display functions for
// rendering session tree panels.
// ============================================================================

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

export type {
  SessionHandle,
  SessionMeta,
  SessionTreeEntry,
  SessionTreeNode,
  TreeNavigationResult,
} from "./session-types.js";
