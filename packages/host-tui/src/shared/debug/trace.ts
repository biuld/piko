// ============================================================================
// shared/debug/trace — diagnostic tracing (pure utility, no protocol deps)
// ============================================================================

export type DebugTraceLevel = "debug" | "info" | "warning" | "error";
export type DebugTraceOutcome = "completed" | "aborted" | "error" | "timeout";

/**
 * Deliberately payload-free diagnostic record. Do not add prompts, tool
 * arguments, model output, credentials, or error messages here.
 */
export interface DebugTraceRecord {
  timestamp: string;
  monotonicMs: number;
  stage: string;
  level: DebugTraceLevel;
  spanId?: string;
  runId?: string;
  taskId?: string;
  agentId?: string;
  stepId?: string;
  toolCallId?: string;
  toolName?: string;
  eventType?: string;
  eventSeq?: number;
  durationMs?: number;
  thresholdMs?: number;
  signalAborted?: boolean;
  outcome?: DebugTraceOutcome;
  status?: string;
  count?: number;
}

export type DebugTraceInput = Omit<DebugTraceRecord, "timestamp" | "monotonicMs" | "level"> & {
  level?: DebugTraceLevel;
};

export type DebugTraceSink = (record: DebugTraceRecord) => void;

let sink: DebugTraceSink | undefined;
let nextSpanId = 0;
const watchdogThresholds = [5_000, 30_000, 120_000] as const;

export function setDebugTraceSink(next: DebugTraceSink | undefined): void {
  sink = next;
}

export function isDebugTraceEnabled(): boolean {
  return sink !== undefined;
}

export function debugTrace(input: DebugTraceInput): void {
  const current = sink;
  if (!current) return;
  try {
    current({
      ...input,
      timestamp: new Date().toISOString(),
      monotonicMs: performance.now(),
      level: input.level ?? "info",
    });
  } catch {
    // Diagnostics must never affect the runtime being diagnosed.
  }
}

export interface DebugSpan {
  readonly spanId?: string;
  end(fields?: Partial<DebugTraceInput>): void;
}

export function startDebugSpan(stage: string, fields: Partial<DebugTraceInput> = {}): DebugSpan {
  if (!sink) return { end: () => {} };

  const spanId = `span-${++nextSpanId}`;
  const startedAt = performance.now();
  let ended = false;
  const base = { ...fields, stage, spanId } as DebugTraceInput;
  debugTrace(base);

  const timers = watchdogThresholds.map((thresholdMs) => {
    const timer = setTimeout(() => {
      if (ended) return;
      debugTrace({
        ...base,
        stage: `${stage}.watchdog`,
        level: "warning",
        durationMs: Math.round(performance.now() - startedAt),
        thresholdMs,
      });
    }, thresholdMs);
    if (typeof timer === "object" && timer && "unref" in timer) {
      (timer as { unref?: () => void }).unref?.();
    }
    return timer;
  });

  return {
    spanId,
    end(extra = {}) {
      if (ended) return;
      ended = true;
      for (const timer of timers) clearTimeout(timer);
      debugTrace({
        ...base,
        ...extra,
        stage: `${stage}.end`,
        durationMs: Math.round(performance.now() - startedAt),
      });
    },
  };
}
