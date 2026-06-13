import type { Message } from "piko-orchestrator-protocol";

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

/** Compute context window usage as a percentage */
export function getContextPercent(totalInputTokens: number, contextWindow: number): number {
  if (!contextWindow || contextWindow <= 0) return 0;
  return (totalInputTokens / contextWindow) * 100;
}
