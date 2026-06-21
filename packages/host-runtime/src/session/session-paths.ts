import { mkdirp } from "../utils/bun-fs.js";
import { joinPath, resolvePath } from "../utils/bun-path.js";

export const CURRENT_SESSION_VERSION = 3;

export function getPikoDir(): string {
  const home = process.env.HOME ?? process.env.USERPROFILE ?? ".";
  return `${home}/.piko`;
}

export function getAgentDir(): string {
  return joinPath(getPikoDir(), "agent");
}

export function getSessionsDir(): string {
  return joinPath(getAgentDir(), "sessions");
}

export async function ensurePikoDir(): Promise<string> {
  const agentDir = getAgentDir();
  const sessionsDir = getSessionsDir();
  await mkdirp(agentDir);
  await mkdirp(sessionsDir);
  return agentDir;
}

export function encodeCwd(cwd: string): string {
  const resolved = resolvePath(cwd);
  return `--${resolved.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-")}--`;
}

export function getSessionDir(cwd: string = process.cwd()): string {
  return joinPath(getSessionsDir(), encodeCwd(cwd));
}

export async function ensureSessionDir(cwd: string): Promise<string> {
  await ensurePikoDir();
  const dir = getSessionDir(cwd);
  await mkdirp(dir);
  return dir;
}

export function generateEntryId(index: number): string {
  return index.toString(16).padStart(8, "0").slice(-8);
}
