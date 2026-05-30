import * as fs from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Message } from "piko-engine-protocol";

const CURRENT_SESSION_VERSION = 1;

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

export function getPikoDir(): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? ".";
  return `${home}/.piko`;
}

export function getAgentDir(): string {
  return join(getPikoDir(), "agent");
}

export function getSessionsDir(): string {
  return join(getAgentDir(), "sessions");
}

export async function ensurePikoDir(): Promise<string> {
  const agentDir = getAgentDir();
  const sessionsDir = getSessionsDir();
  await fs.mkdir(agentDir, { recursive: true });
  await fs.mkdir(sessionsDir, { recursive: true });
  return agentDir;
}

export function encodeCwd(cwd: string): string {
  const resolved = resolve(cwd);
  return `--${resolved.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-")}--`;
}

export function getSessionDir(cwd: string = process.cwd()): string {
  return join(getSessionsDir(), encodeCwd(cwd));
}

async function ensureSessionDir(cwd: string): Promise<string> {
  await ensurePikoDir();
  const dir = getSessionDir(cwd);
  await fs.mkdir(dir, { recursive: true });
  return dir;
}

function generateEntryId(index: number): string {
  return `entry-${index.toString(36)}`;
}

export function parseSessionEntries(content: string): FileEntry[] {
  return content
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as FileEntry);
}

export async function readSessionEntries(path: string): Promise<FileEntry[]> {
  try {
    const data = await fs.readFile(path, "utf-8");
    return parseSessionEntries(data);
  } catch {
    return [];
  }
}

async function findSessionFileById(
  sessionId: string,
  cwd: string = process.cwd(),
): Promise<string | null> {
  const dir = getSessionDir(cwd);
  try {
    const entries = await fs.readdir(dir);
    const matches = entries
      .filter((name) => name.endsWith(".jsonl"))
      .filter((name) => name.includes(sessionId));
    if (matches.length === 0) return null;
    matches.sort();
    return join(dir, matches[matches.length - 1]!);
  } catch {
    return null;
  }
}

export async function findMostRecentSession(
  cwd: string = process.cwd(),
): Promise<SessionHandle | null> {
  const sessions = await listSessions(cwd);
  const latest = sessions[0];
  if (!latest) return null;
  return {
    id: latest.id,
    path: latest.path,
    cwd: latest.cwd,
  };
}

export async function resolveSession(
  specifier: string,
  cwd: string = process.cwd(),
): Promise<SessionHandle | null> {
  if (specifier.endsWith(".jsonl") || specifier.includes("/")) {
    const path = resolve(specifier);
    const meta = await buildSessionMeta(path);
    if (!meta) return null;
    return {
      id: meta.id,
      path: meta.path,
      cwd: meta.cwd,
    };
  }

  const sessions = await listSessions(cwd);
  const exact = sessions.find((session) => session.id === specifier);
  if (exact) {
    return {
      id: exact.id,
      path: exact.path,
      cwd: exact.cwd,
    };
  }

  const partialMatches = sessions.filter((session) => session.id.includes(specifier));
  if (partialMatches.length === 0) return null;
  partialMatches.sort((a, b) => b.modified.localeCompare(a.modified));
  const match = partialMatches[0]!;
  return {
    id: match.id,
    path: match.path,
    cwd: match.cwd,
  };
}

async function buildSessionMeta(path: string): Promise<SessionMeta | null> {
  const entries = await readSessionEntries(path);
  if (entries.length === 0) return null;

  const header = entries[0];
  if (!header || header.type !== "session") return null;

  const messageEntries = entries.filter(
    (entry): entry is SessionMessageEntry => entry.type === "message",
  );
  const modelEntries = entries.filter(
    (entry): entry is ModelChangeEntry => entry.type === "model_change",
  );
  const sessionInfoEntries = entries.filter(
    (entry): entry is SessionInfoEntry => entry.type === "session_info",
  );
  const firstUser = messageEntries.find((entry) => entry.message.role === "user");
  const preview = firstUser ? extractText(firstUser.message).slice(0, 80) : "";
  const latestModelEntry = modelEntries[modelEntries.length - 1];
  const latestSessionInfoEntry = sessionInfoEntries[sessionInfoEntries.length - 1];
  const model = latestModelEntry?.modelId ?? "unknown";
  const stats = await fs.stat(path);

  return {
    id: header.id,
    path,
    cwd: header.cwd,
    parentSessionPath: header.parentSession,
    ...(latestSessionInfoEntry?.name ? { name: latestSessionInfoEntry.name } : {}),
    created: header.timestamp,
    modified: stats.mtime.toISOString(),
    model,
    messageCount: messageEntries.length,
    preview,
  };
}

export async function readSessionMeta(
  sessionId: string,
  cwd: string = process.cwd(),
): Promise<SessionMeta | null> {
  const path = await findSessionFileById(sessionId, cwd);
  if (!path) return null;
  return buildSessionMeta(path);
}

export async function loadSession(
  sessionId: string,
  cwd: string = process.cwd(),
): Promise<Message[]> {
  const path = await findSessionFileById(sessionId, cwd);
  if (!path) return [];
  const entries = await readSessionEntries(path);
  return entries
    .filter((entry): entry is SessionMessageEntry => entry.type === "message")
    .map((entry) => entry.message);
}

export async function loadSessionFromPath(path: string): Promise<Message[]> {
  const entries = await readSessionEntries(path);
  return entries
    .filter((entry): entry is SessionMessageEntry => entry.type === "message")
    .map((entry) => entry.message);
}

function createSessionHeader(sessionId: string, cwd: string, parentSession?: string): SessionHeader {
  return {
    type: "session",
    version: CURRENT_SESSION_VERSION,
    id: sessionId,
    timestamp: new Date().toISOString(),
    cwd: resolve(cwd),
    ...(parentSession ? { parentSession: resolve(parentSession) } : {}),
  };
}

function buildMessageEntries(
  messages: Message[],
  startIndex: number,
  parentId: string | null,
): SessionMessageEntry[] {
  let currentParentId = parentId;
  const entries: SessionMessageEntry[] = [];
  for (const [offset, message] of messages.entries()) {
    const entryId = generateEntryId(startIndex + offset);
    entries.push({
      type: "message",
      id: entryId,
      parentId: currentParentId,
      timestamp: new Date(
        typeof message.timestamp === "number" ? message.timestamp : Date.now(),
      ).toISOString(),
      message,
    });
    currentParentId = entryId;
  }
  return entries;
}

function getLastSessionEntryId(entries: SessionEntry[]): string | null {
  return entries[entries.length - 1]?.id ?? null;
}

function buildSessionPath(sessionId: string, cwd: string): Promise<string> {
  return ensureSessionDir(cwd).then((dir) => join(
    dir,
    `${new Date().toISOString().replace(/[:.]/g, "-")}_${sessionId}.jsonl`,
  ));
}

async function persistSessionFile(
  path: string,
  header: SessionHeader,
  sessionEntries: SessionEntry[],
): Promise<void> {
  const lines = [header, ...sessionEntries]
    .map((entry) => JSON.stringify(entry))
    .join("\n") + "\n";
  await fs.writeFile(path, lines);
}

export async function writeSessionSnapshot(
  sessionId: string,
  entries: SessionEntry[],
  cwd: string = process.cwd(),
  options: WriteSessionSnapshotOptions = {},
): Promise<string> {
  const path = options.sessionPath ?? await buildSessionPath(sessionId, cwd);
  const header = createSessionHeader(sessionId, cwd, options.parentSession);
  await persistSessionFile(path, header, entries);
  return path;
}

export async function appendSessionInfo(
  sessionId: string,
  cwd: string = process.cwd(),
  name?: string,
  sessionPath?: string,
  parentId?: string | null,
  parentSession?: string,
): Promise<AppendSessionMessagesResult> {
  const path = sessionPath ?? await findSessionFileById(sessionId, cwd) ?? await buildSessionPath(sessionId, cwd);
  const existingEntries = await readSessionEntries(path);
  const header = existingEntries[0]?.type === "session"
    ? existingEntries[0]
    : createSessionHeader(sessionId, cwd, parentSession);
  const sessionEntries = existingEntries.filter(
    (entry): entry is SessionEntry => entry.type !== "session",
  );
  const entryId = generateEntryId(sessionEntries.length);
  const sessionInfoEntry: SessionInfoEntry = {
    type: "session_info",
    id: entryId,
    parentId: parentId ?? getLastSessionEntryId(sessionEntries),
    timestamp: new Date().toISOString(),
    ...(name ? { name } : {}),
  };
  await persistSessionFile(path, header, [...sessionEntries, sessionInfoEntry]);
  return {
    path,
    lastEntryId: entryId,
  };
}

export async function appendSessionMessages(
  sessionId: string,
  modelId: string,
  messages: Message[],
  cwd: string = process.cwd(),
  sessionPath?: string,
  parentId?: string | null,
  parentSession?: string,
): Promise<AppendSessionMessagesResult> {
  const path = sessionPath ?? await findSessionFileById(sessionId, cwd) ?? await buildSessionPath(sessionId, cwd);
  const existingEntries = await readSessionEntries(path);
  const header = existingEntries[0]?.type === "session"
    ? existingEntries[0]
    : createSessionHeader(sessionId, cwd, parentSession);

  const sessionEntries = existingEntries.filter(
    (entry): entry is SessionEntry => entry.type !== "session",
  );
  let nextIndex = sessionEntries.length;
  let currentParentId = parentId ?? getLastSessionEntryId(sessionEntries);
  const latestModelId = [...sessionEntries]
    .reverse()
    .find((entry): entry is ModelChangeEntry => entry.type === "model_change")
    ?.modelId;

  const appendedEntries: SessionEntry[] = [];
  if (latestModelId !== modelId) {
    const modelEntryId = generateEntryId(nextIndex);
    appendedEntries.push({
      type: "model_change",
      id: modelEntryId,
      parentId: currentParentId,
      timestamp: new Date().toISOString(),
      modelId,
    });
    currentParentId = modelEntryId;
    nextIndex++;
  }

  appendedEntries.push(...buildMessageEntries(messages, nextIndex, currentParentId));
  await persistSessionFile(path, header, [...sessionEntries, ...appendedEntries]);
  return {
    path,
    lastEntryId: appendedEntries[appendedEntries.length - 1]?.id ?? currentParentId,
  };
}

export async function saveSession(
  sessionId: string,
  modelId: string,
  messages: Message[],
  cwd: string = process.cwd(),
): Promise<void> {
  const existing = await loadSession(sessionId, cwd);
  const newMessages = messages.slice(existing.length);
  if (existing.length === 0 && messages.length === 0) {
    await appendSessionMessages(sessionId, modelId, [], cwd);
    return;
  }
  if (newMessages.length === 0) return;
  await appendSessionMessages(sessionId, modelId, newMessages, cwd);
}

export async function listSessions(cwd: string = process.cwd()): Promise<SessionMeta[]> {
  const dir = getSessionDir(cwd);
  if (!existsSync(dir)) return [];

  const files = (await fs.readdir(dir))
    .filter((name) => name.endsWith(".jsonl"))
    .map((name) => join(dir, name));
  const metas = (await Promise.all(files.map((file) => buildSessionMeta(file))))
    .filter((meta): meta is SessionMeta => meta !== null);
  metas.sort((a, b) => b.modified.localeCompare(a.modified));
  return metas;
}

export async function listAllSessions(): Promise<SessionMeta[]> {
  const sessionsDir = getSessionsDir();
  if (!existsSync(sessionsDir)) return [];

  const directoryEntries = await fs.readdir(sessionsDir, { withFileTypes: true });
  const sessionDirs = directoryEntries
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(sessionsDir, entry.name));

  const metas = (await Promise.all(sessionDirs.map(async (dir) => {
    const files = (await fs.readdir(dir))
      .filter((name) => name.endsWith(".jsonl"))
      .map((name) => join(dir, name));
    return (await Promise.all(files.map((file) => buildSessionMeta(file))))
      .filter((meta): meta is SessionMeta => meta !== null);
  }))).flat();

  metas.sort((a, b) => b.modified.localeCompare(a.modified));
  return metas;
}

export async function deleteSession(sessionPath: string): Promise<void> {
  await fs.unlink(resolve(sessionPath));
}

function extractText(msg: Message): string {
  if (typeof msg.content === "string") return msg.content;
  if (Array.isArray(msg.content)) {
    return msg.content
      .filter((c): c is { type: "text"; text: string } => "text" in c)
      .map((c) => c.text)
      .join(" ");
  }
  return "";
}
