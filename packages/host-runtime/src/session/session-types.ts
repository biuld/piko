import type { Message } from "piko-engine-protocol";

export const CURRENT_SESSION_VERSION = 3;

export interface SessionHeader {
  type: "session";
  version: number;
  id: string;
  timestamp: string;
  cwd: string;
  parentSession?: string;
}

export interface SessionEntryBase {
  type: string;
  id: string;
  parentId: string | null;
  timestamp: string;
}

export interface SessionMessageEntry extends SessionEntryBase {
  type: "message";
  message: Message;
}

export interface ModelChangeEntry extends SessionEntryBase {
  type: "model_change";
  modelId: string;
}

export interface SessionInfoEntry extends SessionEntryBase {
  type: "session_info";
  name?: string;
}

export type SessionEntry = SessionMessageEntry | ModelChangeEntry | SessionInfoEntry;
export type FileEntry = SessionHeader | SessionEntry;

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

export interface AppendSessionMessagesResult {
  path: string;
  lastEntryId: string | null;
}

export interface WriteSessionSnapshotOptions {
  sessionPath?: string;
  parentSession?: string;
}
