export type { ApprovalDecision, ApprovalHandler } from "./approval-controller.js";
export {
  createApprovalResolution,
  createAutoAcceptHandler,
  createAutoDeclineHandler,
} from "./approval-controller.js";
export type {
  SessionEntry,
  SessionHandle,
  SessionMessageEntry,
  SessionMeta,
} from "./file-session-store.js";
export {
  appendSessionInfo,
  deleteSession,
  ensurePikoDir,
  findMostRecentSession,
  getPikoDir,
  listAllSessions,
  listSessions,
  loadSession,
  loadSessionFromPath,
  readSessionMeta,
  resolveSession,
  saveSession,
} from "./file-session-store.js";
export type {
  HostRunResult,
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
} from "./host.js";
export { PikoHost } from "./host.js";
export type { HostConfig } from "./model-config.js";
export { createDefaultSettings, createHostConfig } from "./model-config.js";
export { findModel, listAvailableModels } from "./model-loader.js";
export type { RunResult, SchedulerOptions } from "./scheduler.js";
export { runScheduler } from "./scheduler.js";
export { SessionManager } from "./session-manager.js";
export type {
  CreateSessionRuntimeOptions,
  ReplaceSessionEvent,
  SessionReplaceReason,
  SessionRuntimeDiagnostic,
} from "./session-runtime.js";
export { PikoSessionRuntime, SessionImportFileNotFoundError } from "./session-runtime.js";
export type { SessionRunState, SessionState } from "./session-store.js";
export {
  addUserMessage,
  appendMessages,
  createSession,
  updateSessionState,
} from "./session-store.js";

// Re-export engine-native adapters for backward compat
