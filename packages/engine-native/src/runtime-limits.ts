import type { EngineRuntimeCounters, EngineRuntimeLimits } from "piko-engine-protocol";

export function createCounters(): EngineRuntimeCounters {
  return {
    modelCalls: 0,
    toolCalls: 0,
    approvalRequests: 0,
    consecutiveErrors: 0,
    startedAt: Date.now(),
  };
}

export interface LimitCheckResult {
  exceeded: boolean;
  stopReason: string;
}

/**
 * Check runtime limits before a new model call.
 * Returns the limit that was exceeded, or null if within limits.
 */
export function checkBeforeModelCall(
  counters: EngineRuntimeCounters,
  limits?: EngineRuntimeLimits,
): LimitCheckResult | null {
  if (!limits) return null;

  if (limits.maxModelCalls !== undefined && counters.modelCalls >= limits.maxModelCalls) {
    return { exceeded: true, stopReason: "max_steps" };
  }

  if (
    limits.maxWallClockMs !== undefined &&
    Date.now() - counters.startedAt >= limits.maxWallClockMs
  ) {
    return { exceeded: true, stopReason: "abort" };
  }

  if (
    limits.maxConsecutiveErrors !== undefined &&
    counters.consecutiveErrors >= limits.maxConsecutiveErrors
  ) {
    return { exceeded: true, stopReason: "error" };
  }

  return null;
}

/**
 * Check runtime limits before a new tool call.
 */
export function checkBeforeToolCall(
  counters: EngineRuntimeCounters,
  limits?: EngineRuntimeLimits,
): LimitCheckResult | null {
  if (!limits) return null;

  if (limits.maxToolCalls !== undefined && counters.toolCalls >= limits.maxToolCalls) {
    return { exceeded: true, stopReason: "max_steps" };
  }

  if (
    limits.maxWallClockMs !== undefined &&
    Date.now() - counters.startedAt >= limits.maxWallClockMs
  ) {
    return { exceeded: true, stopReason: "abort" };
  }

  return null;
}

/**
 * Check runtime limits before an approval request.
 */
export function checkBeforeApproval(
  counters: EngineRuntimeCounters,
  limits?: EngineRuntimeLimits,
): LimitCheckResult | null {
  if (!limits) return null;

  if (
    limits.maxApprovalRequests !== undefined &&
    counters.approvalRequests >= limits.maxApprovalRequests
  ) {
    return { exceeded: true, stopReason: "max_steps" };
  }

  return null;
}

/**
 * Wrap a tool executor with a per-tool timeout.
 * Returns a promise that rejects if the executor takes longer than timeoutMs.
 */
export async function withToolTimeout<T>(
  executor: () => Promise<T>,
  timeoutMs?: number,
  signal?: AbortSignal,
): Promise<T> {
  if (!timeoutMs && !signal) return executor();

  return new Promise<T>((resolve, reject) => {
    let timer: ReturnType<typeof setTimeout> | undefined;

    if (timeoutMs) {
      timer = setTimeout(() => {
        reject(new Error(`Tool execution timed out after ${timeoutMs}ms`));
      }, timeoutMs);
    }

    if (signal) {
      if (signal.aborted) {
        clearTimeout(timer);
        reject(new Error("Aborted"));
        return;
      }
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timer);
          reject(new Error("Aborted"));
        },
        { once: true },
      );
    }

    executor()
      .then((result) => {
        clearTimeout(timer);
        resolve(result);
      })
      .catch((err) => {
        clearTimeout(timer);
        reject(err);
      });
  });
}
