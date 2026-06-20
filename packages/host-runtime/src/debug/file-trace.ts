import { appendFile, mkdir } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { type DebugTraceRecord, debugTrace, setDebugTraceSink } from "piko-orchestrator-protocol";

function enabled(value: string | undefined): boolean {
  return value === "1" || value === "true";
}

/** Install a best-effort JSONL trace sink when PIKO_DEBUG is enabled. */
export function installDebugTraceFromEnv(env: NodeJS.ProcessEnv = process.env): string | undefined {
  if (!enabled(env.PIKO_DEBUG)) return undefined;

  const stamp = new Date().toISOString().replaceAll(":", "-");
  const path =
    env.PIKO_DEBUG_LOG ?? join(homedir(), ".piko", "logs", `piko-${stamp}-${process.pid}.jsonl`);
  let writes: Promise<unknown> = mkdir(dirname(path), { recursive: true });

  setDebugTraceSink((record: DebugTraceRecord) => {
    const line = `${JSON.stringify({ pid: process.pid, ...record })}\n`;
    writes = writes.then(() => appendFile(path, line, "utf8")).catch(() => {});
  });
  debugTrace({ stage: "debug.trace.installed" });

  return path;
}
