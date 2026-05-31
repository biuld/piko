// ============================================================================
// Session tree utilities (pure functions)
// ============================================================================
export type { SessionTreeNode } from "./session-types.js";
export { buildSessionTree, getEntryLabel, getSearchableText } from "./session-tree-utils.js";

// ============================================================================
// SessionManager (delegates to pi-agent-core Session)
// ============================================================================
export { SessionManager } from "./session-manager.js";
// listSessions / listAllSessions are now on SessionManager static methods

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

// ============================================================================
// Pi-agent-core session (for advanced use)
// ============================================================================
export type { Session } from "./pi/session.js";
export { JsonlSessionRepo } from "./pi/jsonl-repo.js";
export { NodeExecutionEnv } from "./pi/nodejs-fs.js";
