/**
 * Session entry types — adapted from @earendil-works/pi-agent-core harness/types.ts.
 *
 * Full SessionTreeEntry union, forward-compatible with pi ecosystem.
 */

import type { ImageContent, Message, TextContent } from "piko-engine-protocol";

// ============================================================================
// Base
// ============================================================================

export interface SessionTreeEntryBase {
  type: string;
  id: string;
  parentId: string | null;
  timestamp: string;
}

// ============================================================================
// Entry types
// ============================================================================

export interface MessageEntry extends SessionTreeEntryBase {
  type: "message";
  message: Message;
}

export interface ThinkingLevelChangeEntry extends SessionTreeEntryBase {
  type: "thinking_level_change";
  thinkingLevel: string;
}

export interface ModelChangeEntry extends SessionTreeEntryBase {
  type: "model_change";
  provider: string;
  modelId: string;
}

export interface ActiveToolsChangeEntry extends SessionTreeEntryBase {
  type: "active_tools_change";
  activeToolNames: string[];
}

export interface CompactionEntry<T = unknown> extends SessionTreeEntryBase {
  type: "compaction";
  summary: string;
  firstKeptEntryId: string;
  tokensBefore: number;
  details?: T;
  fromHook?: boolean;
}

export interface BranchSummaryEntry<T = unknown> extends SessionTreeEntryBase {
  type: "branch_summary";
  fromId: string;
  summary: string;
  details?: T;
  fromHook?: boolean;
}

export interface CustomEntry<T = unknown> extends SessionTreeEntryBase {
  type: "custom";
  customType: string;
  data?: T;
}

export interface CustomMessageEntry<T = unknown> extends SessionTreeEntryBase {
  type: "custom_message";
  customType: string;
  content: string | (TextContent | ImageContent)[];
  details?: T;
  display: boolean;
}

export interface LabelEntry extends SessionTreeEntryBase {
  type: "label";
  targetId: string;
  label: string | undefined;
}

export interface SessionInfoEntry extends SessionTreeEntryBase {
  type: "session_info";
  name?: string;
}

export interface LeafEntry extends SessionTreeEntryBase {
  type: "leaf";
  targetId: string | null;
}

// ============================================================================
// Union
// ============================================================================

export type SessionTreeEntry =
  | MessageEntry
  | ThinkingLevelChangeEntry
  | ModelChangeEntry
  | ActiveToolsChangeEntry
  | CompactionEntry
  | BranchSummaryEntry
  | CustomEntry
  | CustomMessageEntry
  | LabelEntry
  | SessionInfoEntry
  | LeafEntry;

/** @deprecated Use SessionTreeEntry */
export type SessionEntry = SessionTreeEntry;

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

export type FileEntry = SessionHeader | SessionTreeEntry;

// ============================================================================
// Metadata
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
  entry: SessionTreeEntry;
  children: SessionTreeNode[];
  label?: string;
}

// ============================================================================
// Session context
// ============================================================================

export interface SessionContext {
  messages: Message[];
  thinkingLevel: string;
  model: { provider: string; modelId: string } | null;
  activeToolNames: string[] | null;
}

// ============================================================================
// Legacy compat
// ============================================================================

/** @deprecated Use MessageEntry */
export type SessionMessageEntry = MessageEntry;
/** @deprecated Use SessionTreeEntryBase */
export type SessionEntryBase = SessionTreeEntryBase;

export const CURRENT_SESSION_VERSION = 3;

export interface WriteSessionSnapshotOptions {
  sessionPath?: string;
  parentSession?: string;
}

export interface AppendSessionMessagesResult {
  path: string;
  lastEntryId: string | null;
}
