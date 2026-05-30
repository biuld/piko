import { execSync } from "node:child_process";
import type { Message } from "piko-engine-protocol";

// ============================================================================
// Token usage
// ============================================================================

export interface CumulativeUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  cost: number;
}

/** Sum usage across all assistant messages */
export function computeCumulativeUsage(messages: Message[]): CumulativeUsage {
  let input = 0;
  let output = 0;
  let cacheRead = 0;
  let cacheWrite = 0;
  let cost = 0;
  for (const msg of messages) {
    if (msg.role === "assistant") {
      const usage = (
        msg as {
          usage?: {
            input?: number;
            output?: number;
            cacheRead?: number;
            cacheWrite?: number;
            cost?: { total?: number };
          };
        }
      ).usage;
      if (usage) {
        input += usage.input ?? 0;
        output += usage.output ?? 0;
        cacheRead += usage.cacheRead ?? 0;
        cacheWrite += usage.cacheWrite ?? 0;
        cost += usage.cost?.total ?? 0;
      }
    }
  }
  return { input, output, cacheRead, cacheWrite, cost };
}

// ============================================================================
// Context usage
// ============================================================================

/** Compute context window usage as a percentage */
export function getContextPercent(totalInputTokens: number, contextWindow: number): number {
  if (!contextWindow || contextWindow <= 0) return 0;
  return (totalInputTokens / contextWindow) * 100;
}

// ============================================================================
// Git
// ============================================================================

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
