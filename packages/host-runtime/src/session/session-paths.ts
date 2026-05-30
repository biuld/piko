import * as fs from "node:fs/promises";
import { join, resolve } from "node:path";

export const CURRENT_SESSION_VERSION = 3;

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

export async function ensureSessionDir(cwd: string): Promise<string> {
  await ensurePikoDir();
  const dir = getSessionDir(cwd);
  await fs.mkdir(dir, { recursive: true });
  return dir;
}

export function generateEntryId(index: number): string {
  return index.toString(16).padStart(8, "0").slice(-8);
}
