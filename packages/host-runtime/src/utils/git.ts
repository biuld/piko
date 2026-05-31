import { execSync } from "node:child_process";

let cachedBranch: { cwd: string; branch: string | undefined } | null = null;

/** Get the current git branch for a working directory (cached per cwd) */
export function getGitBranch(cwd: string): string | undefined {
  if (cachedBranch?.cwd === cwd) return cachedBranch.branch;
  try {
    const branch = execSync("git branch --show-current", {
      cwd,
      encoding: "utf-8",
      timeout: 2000,
    }).trim();
    cachedBranch = { cwd, branch: branch || undefined };
    return cachedBranch.branch;
  } catch {
    cachedBranch = { cwd, branch: undefined };
    return undefined;
  }
}
