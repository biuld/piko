// ============================================================================
// @piko/shared — protocol types plus pure utilities for host-tui.
//
// Host runtime state such as auth, model discovery, settings storage, tools,
// compaction, and sessions is owned by hostd. Keep this module free of host
// runtime implementations.
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

// ---- Session utilities ----

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
  encodeCwd,
  ensurePikoDir,
  flattenSessionTree,
  getAgentDir,
  getEntryLabel,
  getEntrySegments,
  getPikoDir,
  getSearchableText,
  getSessionDir,
  getSessionsDir,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "./session/index.js";

// ---- Utility types & functions (TUI-only, no host-side concerns) ----

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
