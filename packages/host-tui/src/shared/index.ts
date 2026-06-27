// ============================================================================
// @piko/shared — protocol types plus pure utilities for host-tui.
//
// Host runtime state such as auth, model discovery, settings storage, tools,
// compaction, sessions, and filesystem access is owned by hostd.
// This module retains only protocol type mirrors and pure display utilities.
// ============================================================================

// ---- Shared debug ----

export { installDebugTraceFromEnv } from "./debug/file-trace.js";

// ---- Context files (inline type) ----

export interface ContextFile {
  path: string;
  content: string;
}

// ---- Protocol re-exports ----

export * from "./orchd/protocol/index.js";

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
