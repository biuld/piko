import * as fs from "node:fs/promises";
import { existsSync } from "node:fs";
import type { Message } from "piko-engine-protocol";

// ---- Directory ----

export function getPikoDir(): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? ".";
  return `${home}/.piko`;
}

export async function ensurePikoDir(): Promise<string> {
  const dir = getPikoDir();
  await fs.mkdir(dir, { recursive: true });
  await fs.mkdir(`${dir}/sessions`, { recursive: true });
  return dir;
}

// ---- Session metadata ----

export interface SessionMeta {
  id: string;
  created: string;
  modified: string;
  model: string;
  messageCount: number;
  preview: string; // First few words of first user message
}

export async function readSessionMeta(sessionId: string): Promise<SessionMeta | null> {
  const dir = getPikoDir();
  const file = `${dir}/sessions/${sessionId}/meta.json`;
  try {
    const data = await fs.readFile(file, "utf-8");
    return JSON.parse(data);
  } catch {
    return null;
  }
}

async function writeSessionMeta(meta: SessionMeta): Promise<void> {
  const dir = getPikoDir();
  const sessionDir = `${dir}/sessions/${meta.id}`;
  await fs.mkdir(sessionDir, { recursive: true });
  await fs.writeFile(`${sessionDir}/meta.json`, JSON.stringify(meta, null, 2));
}

// ---- Messages ----

export async function loadSession(sessionId: string): Promise<Message[]> {
  const dir = getPikoDir();
  const file = `${dir}/sessions/${sessionId}/messages.jsonl`;
  try {
    const data = await fs.readFile(file, "utf-8");
    return data.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  } catch {
    return [];
  }
}

export async function saveSession(
  sessionId: string,
  modelId: string,
  messages: Message[],
): Promise<void> {
  const dir = getPikoDir();
  const sessionDir = `${dir}/sessions/${sessionId}`;
  await fs.mkdir(sessionDir, { recursive: true });

  // Write messages as JSONL
  const lines = messages.map((m) => JSON.stringify(m)).join("\n") + "\n";
  await fs.writeFile(`${sessionDir}/messages.jsonl`, lines);

  // Write/update metadata
  const firstUser = messages.find((m) => m.role === "user");
  const preview = firstUser
    ? extractText(firstUser).slice(0, 80)
    : "";

  const existing = await readSessionMeta(sessionId);
  const now = new Date().toISOString();

  await writeSessionMeta({
    id: sessionId,
    created: existing?.created ?? now,
    modified: now,
    model: modelId,
    messageCount: messages.length,
    preview: preview || (existing?.preview ?? ""),
  });
}

// ---- Session listing ----

export async function listSessions(): Promise<SessionMeta[]> {
  const dir = getPikoDir();
  const sessionsDir = `${dir}/sessions`;
  try {
    const entries = await fs.readdir(sessionsDir, { withFileTypes: true });
    const metas: SessionMeta[] = [];
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const meta = await readSessionMeta(entry.name);
        if (meta) metas.push(meta);
      }
    }
    metas.sort((a, b) => b.modified.localeCompare(a.modified));
    return metas;
  } catch {
    return [];
  }
}

// ---- Helpers ----

function extractText(msg: Message): string {
  if (typeof msg.content === "string") return msg.content;
  if (Array.isArray(msg.content)) {
    return msg.content
      .filter((c): c is { type: "text"; text: string } => "text" in c)
      .map((c) => (c as { text: string }).text)
      .join(" ");
  }
  return "";
}
