import * as fs from "node:fs/promises";
import type { Message } from "piko-engine-protocol";

export function getPikoDir(): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? ".";
  return `${home}/.piko`;
}

let ensured = false;

export async function ensurePikoDir(): Promise<string> {
  if (ensured) return getPikoDir();
  const dir = getPikoDir();
  await fs.mkdir(dir, { recursive: true });
  await fs.mkdir(`${dir}/sessions`, { recursive: true });
  ensured = true;
  return dir;
}

export async function saveSession(sessionId: string, messages: Message[]): Promise<void> {
  const dir = await ensurePikoDir();
  await fs.writeFile(`${dir}/sessions/${sessionId}.json`, JSON.stringify(messages, null, 2));
}

export async function loadSession(sessionId: string): Promise<Message[]> {
  const dir = getPikoDir();
  try {
    const data = await fs.readFile(`${dir}/sessions/${sessionId}.json`, "utf-8");
    return JSON.parse(data);
  } catch {
    return [];
  }
}

export async function listSessions(): Promise<string[]> {
  const dir = getPikoDir();
  try {
    const files = await fs.readdir(`${dir}/sessions`);
    return files.filter((f) => f.endsWith(".json")).map((f) => f.replace(".json", ""));
  } catch {
    return [];
  }
}
