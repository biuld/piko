/**
 * Session types shared by hostd snapshots and TUI projections.
 *
 * The authoritative Rust shape lives in piko-protocol. These TypeScript types
 * mirror that wire format; TUI must not read the JSONL repo directly.
 */

import type { ImageContent, Message, TextContent } from "../types.js";

// Minimal sidecar types (hostd-owned — defined inline for type compatibility)
interface AgentSessionRecord {
  agentId: string;
  agentSessionId: string;
  kind: "main" | "subagent";
}

interface AgentTaskRecord {
  taskId: string;
  agentId: string;
  status: string;
}

// ============================================================================
// Session tree entry types
// ============================================================================

export interface SessionTreeEntryBase {
  type: string;
  id: string;
  parentId: string | null;
  timestamp: string;
}

export type BashExecutionMessage = {
  role: "bashExecution";
  content?: string | (TextContent | ImageContent)[];
  [key: string]: unknown;
};

export type CustomPersistedMessage = {
  role: "custom";
  content?: string | (TextContent | ImageContent)[];
  customType?: string;
  [key: string]: unknown;
};

export type PersistableMessage = Message | BashExecutionMessage | CustomPersistedMessage;

export interface MessageEntry extends SessionTreeEntryBase {
  type: "message";
  message: PersistableMessage;
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
  entry: SessionTreeEntry;
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

export interface SessionPersistenceOverview {
  rootSessionId: string;
  rootSessionPath: string;
  mainMessageCount: number;
  hasSidecar: boolean;
  agentSessions: AgentSessionRecord[];
  tasks: AgentTaskRecord[];
  subagentCount: number;
  taskCount: number;
}

/** Result of navigating a session tree to a specific entry. */
export type TreeNavigationResult = {
  status: "navigated" | "already_current";
  sessionId: string;
  oldLeafId: string | null;
  newLeafId: string | null;
  selectedEntryId: string;
  branchEntries: SessionTreeEntry[];
  editorContent?: unknown;
};
