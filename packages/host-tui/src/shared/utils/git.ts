let cachedBranch: { cwd: string; branch: string | undefined } | null = null;

/** Get the current git branch for a working directory (cached per cwd) */
export function getGitBranch(cwd: string): string | undefined {
  if (cachedBranch?.cwd === cwd) return cachedBranch.branch;
  try {
    const result = Bun.spawnSync(["git", "branch", "--show-current"], {
      cwd,
      stdin: "ignore",
      stdout: "pipe",
      stderr: "ignore",
      timeout: 2000,
    });
    if (result.exitCode !== 0) {
      cachedBranch = { cwd, branch: undefined };
      return undefined;
    }
    const branch = new TextDecoder().decode(result.stdout).trim();
    cachedBranch = { cwd, branch: branch || undefined };
    return cachedBranch.branch;
  } catch {
    cachedBranch = { cwd, branch: undefined };
    return undefined;
  }
}
