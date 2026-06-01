// ============================================================================
// TUI Actions — side-effect functions dispatched from renderer
// Actions call PikoHost; renderer never calls PikoHost directly.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { PikoHost, SettingsManager } from "piko-host-runtime";
import type { TuiEvent } from "./events.js";
import type { TuiState } from "./state.js";

// ============================================================================
// Action context — dependencies injected into actions
// ============================================================================

export interface ActionContext {
  host: PikoHost;
  settingsManager?: SettingsManager;
  dispatch: (event: TuiEvent) => void;
  getState: () => TuiState;
  /** Current abort controller for the running stream. Set by submitPrompt, cleared on finish. */
  abortController?: AbortController | null;
}

// ============================================================================
// Actions
// ============================================================================

/**
 * Submit user prompt to the engine.
 */
export async function submitPrompt(ctx: ActionContext, text: string): Promise<void> {
  const trimmed = text.trim();
  if (!trimmed) return;

  const ac = new AbortController();
  ctx.abortController = ac;

  ctx.dispatch({ type: "user_submitted", text: trimmed });
  ctx.dispatch({ type: "stream_started" });

  try {
    const stream = ctx.host.streamPrompt(
      trimmed,
      {
        onLifecycleEvent: (e) => {
          if (e.type === "queue_update") {
            ctx.dispatch({
              type: "queue_update",
              steerCount: e.steerCount,
              steerPreview: e.steerPreview,
              followUpCount: e.followUpCount,
              followUpPreview: e.followUpPreview,
            });
          }
        },
      },
      ac.signal,
    );

    let hasAssistant = false;
    const toolNames = new Map<string, string>();

    for await (const event of stream) {
      if (event.type === "message_delta") {
        if (!hasAssistant) hasAssistant = true;
        ctx.dispatch({
          type: "assistant_delta",
          delta: (event as { delta: string }).delta,
        });
      } else if (event.type === "thinking_delta") {
        ctx.dispatch({
          type: "thinking_delta",
          delta: (event as { delta: string }).delta,
        });
      } else if (event.type === "tool_call_start") {
        toolNames.set(event.id, event.name);
        ctx.dispatch({
          type: "tool_call_started",
          id: event.id,
          name: event.name,
          args: event.args,
        });
      } else if (event.type === "tool_call_end") {
        const name = toolNames.get(event.id) ?? event.id;
        ctx.dispatch({
          type: "tool_call_ended",
          id: event.id,
          name,
          result: event.result,
          isError: event.isError,
        });
      }
    }

    const result = await stream.result();
    ctx.abortController = null;
    ctx.dispatch({
      type: "turn_finished",
      status: result.status,
      transcript: result.messages,
    });

    // Update usage
    const u = computeUsage(result.messages);
    const state = ctx.getState();
    ctx.dispatch({
      type: "usage_updated",
      inputTokens: state.usage.inputTokens + u.input,
      outputTokens: state.usage.outputTokens + u.output,
      cacheReadTokens: state.usage.cacheReadTokens + ((u as any).cacheRead ?? 0),
      cacheWriteTokens: state.usage.cacheWriteTokens + ((u as any).cacheWrite ?? 0),
      totalCost: state.usage.totalCost + u.cost,
    });
  } catch (err) {
    ctx.abortController = null;
    if (ac.signal.aborted) {
      ctx.dispatch({
        type: "turn_finished",
        status: "aborted",
        transcript: ctx.getState().transcript as any,
      });
    } else {
      ctx.dispatch({
        type: "turn_failed",
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }
}

/**
 * Abort the current running turn.
 */
export function abortRun(ctx: ActionContext, _signal?: AbortSignal): void {
  ctx.dispatch({ type: "aborted" });
  // The actual abort is handled by the signal passed to streamPrompt
}

/**
 * Change the current model.
 */
export function switchModel(
  ctx: ActionContext,
  model: Model<string>,
  providerConfig: EngineProviderConfig,
): void {
  ctx.dispatch({
    type: "model_changed",
    model,
    providerConfig,
  });
}

/**
 * Change the thinking level.
 */
export function setThinkingLevel(ctx: ActionContext, level: string): void {
  ctx.dispatch({ type: "thinking_level_changed", level });
}

/**
 * Resume a session by ID.
 */
export async function resumeSession(ctx: ActionContext, sessionId: string): Promise<void> {
  // Host handles the actual resume logic
  ctx.dispatch({
    type: "session_resumed",
    sessionId,
    transcript: [],
  });
}

/**
 * Fork a session.
 */
export async function forkSession(ctx: ActionContext, entryId: string): Promise<void> {
  // Host handles the actual fork logic
  ctx.dispatch({ type: "session_forked", sessionId: entryId });
}

// ============================================================================
// Helpers
// ============================================================================

function computeUsage(messages: any[]): {
  input: number;
  output: number;
  cost: number;
} {
  let input = 0;
  let output = 0;
  let cost = 0;

  for (const msg of messages) {
    if (msg.usage) {
      input += msg.usage.inputTokens ?? 0;
      output += msg.usage.outputTokens ?? 0;
      cost += msg.usage.cost ?? 0;
    }
  }

  return { input, output, cost };
}
