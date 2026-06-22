// ============================================================================
// Session tree utilities (pure functions)
// ============================================================================

// ============================================================================
// SessionManager (delegates to pi-agent-core Session)
// ============================================================================
export { SessionManager, type TreeNavigationResult } from "./session-manager.js";
export type {
  AgentContextPolicy,
  AgentPersistencePolicy,
  AgentRuntimeEventRecord,
  AgentSessionRecord,
  AgentTaskRecord,
  AgentTranscriptPolicy,
  PikoSessionSidecarHeader,
  PikoSessionSidecarRecord,
} from "./session-sidecar.js";
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
export type { SessionTreeNode } from "./session-types.js";

// listSessions / listAllSessions are now on SessionManager static methods

export { BunExecutionEnv, NodeExecutionEnv } from "./bun-execution-env.js";
export type { SandboxExecutionEnvOptions } from "./sandbox-execution-env.js";
export { SandboxExecutionEnv } from "./sandbox-execution-env.js";
// ============================================================================
// Session paths
// ============================================================================
export {
  encodeCwd,
  ensurePikoDir,
  getAgentDir,
  getPikoDir,
  getSessionDir,
  getSessionsDir,
} from "./session-paths.js";
// ============================================================================
// PikoSessionRuntime (session switching + event hooks)
// ============================================================================
export type {
  CreateSessionRuntimeOptions,
  ReplaceSessionEvent,
  SessionReplaceReason,
  SessionRuntimeDiagnostic,
} from "./session-runtime.js";
export { PikoSessionRuntime, SessionImportFileNotFoundError } from "./session-runtime.js";
// ============================================================================
// SessionState (in-memory state for scheduler)
// ============================================================================
export type { SessionRunState, SessionState } from "./session-store.js";
export {
  addUserMessage,
  appendMessages,
  createSession,
  updateSessionState,
} from "./session-store.js";
// ============================================================================
// Session types (aligned with pi-agent-core)
// ============================================================================
export type {
  AppendSessionMessagesResult,
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
  WriteSessionSnapshotOptions,
} from "./session-types.js";
