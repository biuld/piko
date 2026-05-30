import * as fs from "node:fs/promises";
import { join, resolve } from "node:path";
import type { Message } from "piko-engine-protocol";
import { findSessionFileById, readSessionEntries } from "./session-meta.js";
import { CURRENT_SESSION_VERSION, ensureSessionDir, generateEntryId } from "./session-paths.js";
import type {
  AppendSessionMessagesResult,
  ModelChangeEntry,
  SessionEntry,
  SessionHeader,
  SessionInfoEntry,
  SessionMessageEntry,
  WriteSessionSnapshotOptions,
} from "./session-types.js";

function createSessionHeader(
  sessionId: string,
  cwd: string,
  parentSession?: string,
): SessionHeader {
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
  return ensureSessionDir(cwd).then((dir) =>
    join(dir, `${new Date().toISOString().replace(/[:.]/g, "-")}_${sessionId}.jsonl`),
  );
}

async function persistSessionFile(
  path: string,
  header: SessionHeader,
  sessionEntries: SessionEntry[],
): Promise<void> {
  const lines = `${[header, ...sessionEntries].map((entry) => JSON.stringify(entry)).join("\n")}\n`;
  await fs.writeFile(path, lines);
}

export async function writeSessionSnapshot(
  sessionId: string,
  entries: SessionEntry[],
  cwd: string = process.cwd(),
  options: WriteSessionSnapshotOptions = {},
): Promise<string> {
  const path = options.sessionPath ?? (await buildSessionPath(sessionId, cwd));
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
  const path =
    sessionPath ??
    (await findSessionFileById(sessionId, cwd)) ??
    (await buildSessionPath(sessionId, cwd));
  const existingEntries = await readSessionEntries(path);
  const header =
    existingEntries[0]?.type === "session"
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
  return { path, lastEntryId: entryId };
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
  const path =
    sessionPath ??
    (await findSessionFileById(sessionId, cwd)) ??
    (await buildSessionPath(sessionId, cwd));
  const existingEntries = await readSessionEntries(path);
  const header =
    existingEntries[0]?.type === "session"
      ? existingEntries[0]
      : createSessionHeader(sessionId, cwd, parentSession);

  const sessionEntries = existingEntries.filter(
    (entry): entry is SessionEntry => entry.type !== "session",
  );
  let nextIndex = sessionEntries.length;
  let currentParentId = parentId ?? getLastSessionEntryId(sessionEntries);
  const latestModelId = [...sessionEntries]
    .reverse()
    .find((entry): entry is ModelChangeEntry => entry.type === "model_change")?.modelId;

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
  const { loadSession } = await import("./session-meta.js");
  const existing = await loadSession(sessionId, cwd);
  const newMessages = messages.slice(existing.length);
  if (existing.length === 0 && messages.length === 0) {
    await appendSessionMessages(sessionId, modelId, [], cwd);
    return;
  }
  if (newMessages.length === 0) return;
  await appendSessionMessages(sessionId, modelId, newMessages, cwd);
}

export async function deleteSession(sessionPath: string): Promise<void> {
  await fs.unlink(resolve(sessionPath));
}

export { parseSessionEntries, readSessionEntries } from "./session-meta.js";
