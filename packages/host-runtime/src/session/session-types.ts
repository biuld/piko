/**
 * Session types — piko-specific extensions + re-exports from piko-session.
 *
 * The canonical SessionTreeEntry union is now in piko-session.
 * This file re-exports it and adds piko-only types (SessionMeta, SessionTreeNode, etc.).
 */

import type { Message } from "piko-orchestrator";

// ============================================================================
// Re-exports from piko-session (canonical session tree entry types)
// ============================================================================

export type {
  ActiveToolsChangeEntry,
  BranchSummaryEntry,
  CompactionEntry,
  CustomEntry,
  CustomMessageEntry,
  LabelEntry,
  LeafEntry,
  MessageEntry,
  ModelChangeEntry,
  SessionInfoEntry,
  SessionTreeEntry,
  SessionTreeEntryBase,
  ThinkingLevelChangeEntry,
} from "piko-session";

// ============================================================================
// Legacy compat
// ============================================================================

import type { MessageEntry, SessionTreeEntry, SessionTreeEntryBase } from "piko-session";

/** @deprecated Use MessageEntry */
export type SessionMessageEntry = MessageEntry;
/** @deprecated Use SessionTreeEntryBase */
export type SessionEntryBase = SessionTreeEntryBase;
/** @deprecated Use SessionTreeEntry */
export type { SessionTreeEntry as SessionEntry };

// ============================================================================
// File format
// ============================================================================

export interface SessionHeader {
  type: "session";
  version: number;
  id: string;
  timestamp: string;
  cwd: string;
  parentSession?: string;
}

export type FileEntry = SessionHeader | import("piko-session").SessionTreeEntry;

// ============================================================================
// Piko metadata types
// ============================================================================

export interface SessionMeta {
  id: string;
  path: string;
  cwd: string;
  parentSessionPath?: string;
  name?: string;
  created: string;
  modified: string;
  model: string;
  messageCount: number;
  preview: string;
}

export interface SessionHandle {
  id: string;
  path: string;
  cwd: string;
}

/** Tree node for session tree display in TUI. */
export interface SessionTreeNode {
  entry: import("piko-session").SessionTreeEntry;
  children: SessionTreeNode[];
  label?: string;
  labelTimestamp?: string;
}

// ============================================================================
// Piko session context (uses Message, not AgentMessage)
// ============================================================================

export interface SessionContext {
  messages: Message[];
  thinkingLevel: string;
  model: { provider: string; modelId: string } | null;
  activeToolNames: string[] | null;
}

// ============================================================================
// Piko-specific constants & options
// ============================================================================

export const CURRENT_SESSION_VERSION = 3;

export interface WriteSessionSnapshotOptions {
  sessionPath?: string;
  parentSession?: string;
}

export interface AppendSessionMessagesResult {
  path: string;
  lastEntryId: string | null;
}
