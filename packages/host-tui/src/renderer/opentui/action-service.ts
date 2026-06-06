// ============================================================================
// ActionService — stable service object holding host, abort controller,
// model registry, and all side-effect action methods.
//
// This replaces the transient ActionContext that was recreated on every
// Solid render, which caused the abort controller to be lost.
// ============================================================================

import type { ImageContent } from "piko-engine-protocol";
import {
  computeCumulativeUsage,
  createHostConfig,
  type ModelRegistry,
  type PikoHost,
  type SettingsManager,
} from "piko-host-runtime";
import type { TuiEvent } from "../../state/events.js";
import type { TuiState } from "../../state/state.js";
import type { TuiStore } from "./store.js";

// ============================================================================
// Service
// ============================================================================

export class ActionService {
  readonly host: PikoHost;
  readonly store: TuiStore;
  readonly modelRegistry?: ModelRegistry;
  readonly settingsManager?: SettingsManager;

  /** Current abort controller for the running stream. Stable across renders. */
  abortController: AbortController | null = null;

  /** Cleanup callback set by the renderer entry point. Called before process exit. */
  private readonly shutdownRuntime?: () => void;

  /** Notification callback — wired by TuiController to NotificationCenter. */
  onNotify?: (message: string, severity?: "info" | "success" | "warning" | "error") => void;

  constructor(
    host: PikoHost,
    store: TuiStore,
    modelRegistry?: ModelRegistry,
    settingsManager?: SettingsManager,
    shutdownRuntime?: () => void,
  ) {
    this.host = host;
    this.store = store;
    this.modelRegistry = modelRegistry;
    this.settingsManager = settingsManager;
    this.shutdownRuntime = shutdownRuntime;

    // Register persistent lifecycle callback on Host.
    // queue_update events flow through here whether triggered by the
    // run loop or by steer() / followUp() / dequeue().
    this.host.setLifecycleCallback((e) => {
      if (e.type === "queue_update") {
        this.dispatch({
          type: "queue_update",
          steerCount: e.steerCount,
          steerPreview: e.steerPreview,
          followUpCount: e.followUpCount,
          followUpPreview: e.followUpPreview,
        });
      }
    });
  }

  private notify(message: string, severity?: "info" | "success" | "warning" | "error"): void {
    this.onNotify?.(message, severity);
  }

  dispatch(event: TuiEvent): void {
    this.store.dispatch(event);
  }

  getState(): TuiState {
    return this.store.state();
  }

  // ==========================================================================
  // Submit prompt
  // ==========================================================================

  async submitPrompt(text: string, images?: ImageContent[]): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;

    // Let Host decide: idle → stream, running → queue
    const streamOrNull = this.host.prompt(trimmed, "auto");

    // Host queued the message (steer/followUp) — no stream to process
    if (!streamOrNull) {
      this.dispatch({ type: "user_submitted", text: trimmed });
      return;
    }

    const ac = new AbortController();
    this.abortController = ac;

    this.store.batchDispatch([
      { type: "user_submitted", text: trimmed },
      { type: "stream_started" },
    ]);

    try {
      const stream = streamOrNull;

      const toolNames = new Map<string, string>();

      for await (const event of stream) {
        if (event.type === "message_delta") {
          this.dispatch({
            type: "assistant_delta",
            delta: (event as { delta: string }).delta,
          });
        } else if (event.type === "thinking_delta") {
          this.dispatch({
            type: "thinking_delta",
            delta: (event as { delta: string }).delta,
          });
        } else if (event.type === "tool_call_start") {
          toolNames.set(event.id, event.name);
          this.dispatch({
            type: "tool_call_started",
            id: event.id,
            name: event.name,
            args: event.args,
          });
        } else if (event.type === "tool_call_end") {
          const name = toolNames.get(event.id) ?? event.id;
          this.dispatch({
            type: "tool_call_ended",
            id: event.id,
            name,
            result: event.result,
            isError: event.isError,
          });
        }
      }

      const result = await stream.result();
      this.abortController = null;

      // Rebuild canonical transcript from engine result
      this.dispatch({
        type: "turn_finished",
        status: result.status,
        transcript: result.messages,
      });

      // Update usage using computeCumulativeUsage
      const u = computeCumulativeUsage(result.messages);
      const state = this.getState();
      this.dispatch({
        type: "usage_updated",
        inputTokens: state.usage.inputTokens + u.input,
        outputTokens: state.usage.outputTokens + u.output,
        cacheReadTokens: state.usage.cacheReadTokens + u.cacheRead,
        cacheWriteTokens: state.usage.cacheWriteTokens + u.cacheWrite,
        totalCost: state.usage.totalCost + u.cost,
      });
    } catch (err) {
      this.abortController = null;
      if (ac.signal.aborted) {
        this.notify("Stream aborted", "warning");
        this.dispatch({
          type: "turn_finished",
          status: "aborted",
          transcript: this.getState().transcript as any,
        });
      } else {
        const errMsg = err instanceof Error ? err.message : String(err);
        this.notify(`Stream error: ${errMsg}`, "error");
        this.dispatch({
          type: "turn_failed",
          error: errMsg,
        });
      }
    }
  }

  // ==========================================================================
  // Abort
  // ==========================================================================

  abortRun(): void {
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    this.dispatch({ type: "aborted" });
  }

  // ==========================================================================
  // Model switching
  // ==========================================================================

  /**
   * Switch to a new model using the ModelRegistry for proper resolution.
   */
  switchModel(modelId: string, providerName: string): boolean {
    if (!this.modelRegistry) return false;

    const resolved = this.modelRegistry.resolve(modelId, providerName);
    if (!resolved) return false;

    const currentConfig = this.host.getConfig();
    this.host.setConfig(
      createHostConfig(
        resolved.model,
        resolved.providerConfig,
        currentConfig.settings,
        currentConfig.tools,
      ),
    );

    this.notify(`Model: ${resolved.model.id}`, "success");
    this.dispatch({
      type: "model_changed",
      model: resolved.model,
      providerConfig: resolved.providerConfig,
    });
    return true;
  }

  /**
   * Change thinking level.
   */
  setThinkingLevel(level: string): void {
    this.host.setThinkingLevel(level);
    this.notify(`Thinking: ${level}`, "info");
    this.dispatch({ type: "thinking_level_changed", level });
  }

  // ==========================================================================
  // Session switching
  // ==========================================================================

  /**
   * Resume/switch to a different session by path or ID.
   */
  async switchSession(specifier: string): Promise<void> {
    const newSession = await this.host.switchSession(specifier);
    if (!newSession) return;

    await this.host.restoreFromSession();
    const config = this.host.getConfig();
    const restoredThinking = this.host.getThinkingLevel();
    const messages = await this.host.loadMessages();
    const entries = await this.host.loadBranchEntries();
    const sessionName = await this.host.getSessionName();

    // Sync restored model/thinking level from session entries into TUI state
    // so the bottom bar reflects the correct values.
    this.dispatch({
      type: "model_changed",
      model: config.model,
      providerConfig: config.provider,
    });
    this.dispatch({
      type: "thinking_level_changed",
      level: restoredThinking,
    });

    // Build timeline transcript: merge message-based view models with
    // metadata entries (model_change, thinking_level_change, etc.)
    const { entriesToTranscript } = await import("../../timeline/entries-to-transcript.js");
    const transcript = entriesToTranscript(entries);

    this.notify(
      `Session: ${sessionName ?? specifier.slice(0, 20)} (${messages.length} messages)`,
      "success",
    );

    this.dispatch({
      type: "session_resumed",
      sessionId: specifier,
      sessionName: sessionName ?? undefined,
      transcript,
    });
  }

  // ==========================================================================
  // Shutdown
  // ==========================================================================

  shutdown(): void {
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    if (this.shutdownRuntime) {
      this.shutdownRuntime();
    } else {
      process.exit(0);
    }
  }
}
