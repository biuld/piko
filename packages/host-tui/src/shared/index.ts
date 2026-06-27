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

// ---- Utility types & functions ----

export type {
  CumulativeUsage,
  FileArgument,
  ImageAttachment,
  ImageDimensions,
  ImageResizeOptions,
  TimingEntry,
} from "./utils/index.js";
export {
  applyHttpSettings,
  basenamePath,
  computeCumulativeUsage,
  configureHttpDispatcher,
  createImageAttachment,
  dirnamePath,
  estimateImageTokens,
  extnamePath,
  getContextPercent,
  getGitBranch,
  getImageDimensions,
  getImageFormatFromPath,
  getTimings,
  isAbsolutePath,
  isImage,
  joinPath,
  parsePath,
  pathSeparator,
  processFileArguments,
  resetTimings,
  resolvePath,
  shouldResize,
  Timings,
} from "./utils/index.js";
