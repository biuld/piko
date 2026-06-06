// ============================================================================
// Session tree utilities (pure functions)
// ============================================================================

// ============================================================================
// SessionManager (delegates to pi-agent-core Session)
// ============================================================================
export { SessionManager } from "./session-manager.js";
export { buildSessionTree, getEntryLabel, getSearchableText } from "./session-tree-utils.js";
export type { SessionTreeNode } from "./session-types.js";

// listSessions / listAllSessions are now on SessionManager static methods

export { NodeExecutionEnv } from "./nodejs-fs.js";
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
  SessionEntry,
  SessionEntryBase,
  SessionHandle,
  SessionHeader,
  SessionInfoEntry,
  SessionMessageEntry,
  SessionMeta,
  SessionTreeEntry,
  WriteSessionSnapshotOptions,
} from "./session-types.js";
