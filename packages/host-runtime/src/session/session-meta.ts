import { existsSync } from "node:fs";
import * as fs from "node:fs/promises";
import { join, resolve } from "node:path";
import type { Message } from "piko-engine-protocol";
import { getSessionDir, getSessionsDir } from "./session-paths.js";
import type {
  FileEntry,
  ModelChangeEntry,
  SessionHandle,
  SessionInfoEntry,
  SessionMessageEntry,
  SessionMeta,
} from "./session-types.js";

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

export async function findSessionFileById(
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
  return { id: latest.id, path: latest.path, cwd: latest.cwd };
}

export async function resolveSession(
  specifier: string,
  cwd: string = process.cwd(),
): Promise<SessionHandle | null> {
  if (specifier.endsWith(".jsonl") || specifier.includes("/")) {
    const path = resolve(specifier);
    const meta = await buildSessionMeta(path);
    if (!meta) return null;
    return { id: meta.id, path: meta.path, cwd: meta.cwd };
  }

  const sessions = await listSessions(cwd);
  const exact = sessions.find((session) => session.id === specifier);
  if (exact) return { id: exact.id, path: exact.path, cwd: exact.cwd };

  const partialMatches = sessions.filter((session) => session.id.includes(specifier));
  if (partialMatches.length === 0) return null;
  partialMatches.sort((a, b) => b.modified.localeCompare(a.modified));
  const match = partialMatches[0]!;
  return { id: match.id, path: match.path, cwd: match.cwd };
}

async function buildSessionMeta(path: string): Promise<SessionMeta | null> {
  const entries = await readSessionEntries(path);
  if (entries.length === 0) return null;

  const header = entries[0];
  if (header?.type !== "session") return null;

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

export async function listSessions(cwd: string = process.cwd()): Promise<SessionMeta[]> {
  const dir = getSessionDir(cwd);
  if (!existsSync(dir)) return [];

  const files = (await fs.readdir(dir))
    .filter((name) => name.endsWith(".jsonl"))
    .map((name) => join(dir, name));
  const metas = (await Promise.all(files.map((file) => buildSessionMeta(file)))).filter(
    (meta): meta is SessionMeta => meta !== null,
  );
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

  const metas = (
    await Promise.all(
      sessionDirs.map(async (dir) => {
        const files = (await fs.readdir(dir))
          .filter((name) => name.endsWith(".jsonl"))
          .map((name) => join(dir, name));
        return (await Promise.all(files.map((file) => buildSessionMeta(file)))).filter(
          (meta): meta is SessionMeta => meta !== null,
        );
      }),
    )
  ).flat();

  metas.sort((a, b) => b.modified.localeCompare(a.modified));
  return metas;
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
