// ============================================================================
// ActionService — stable service object holding host, abort controller,
// model registry, and all side-effect action methods.
//
// This replaces the transient ActionContext that was recreated on every
// Solid render, which caused the abort controller to be lost.
// ============================================================================

import { SessionActions } from "../../actions/session-actions.js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import type { ApprovalStore } from "../../approval-store.js";
import type { HostdClient } from "../../client/index.js";
import type { NotifyInput } from "../../notifications/types.js";
import type {
  ImageContent,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "../../shared/index.js";
import {
  computeCumulativeUsage,
  debugTrace,
  type ModelRegistry,
  type SettingsManager,
  startDebugSpan,
} from "../../shared/index.js";
import type { TuiEvent } from "../../state/events.js";
import type { TuiState } from "../../state/state.js";
import { ApprovalActionController } from "./approval-action-controller.js";
import { HostdActionAdapter } from "./hostd-action-adapter.js";
import { RuntimeConfigAdapter } from "./runtime-config-adapter.js";
import type { TuiStore } from "./store.js";

// ============================================================================
// Service
// ============================================================================

export class ActionService {
  readonly host: TuiHostFacade;
  readonly store: TuiStore;
  readonly modelRegistry?: ModelRegistry;
  readonly settingsManager: SettingsManager;
  readonly session: SessionActions;

  /** Current abort controller for the running stream. Stable across renders. */
  abortController: AbortController | null = null;

  /** Cleanup callback set by the renderer entry point. Called before process exit. */
  private readonly shutdownRuntime?: () => void;
  private readonly hostd: HostdActionAdapter;
  private readonly runtimeConfig: RuntimeConfigAdapter;
  private readonly approval: ApprovalActionController;

  readonly approvalHandler = (
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision> => this.approval.approvalHandler(request, signal);

  resolveApproval(toolEntityId: string, decision: ToolApprovalDecision): void {
    this.approval.resolveApproval(toolEntityId, decision);
  }

  setApprovalBridge(bridge: {
    onPending(
      listener: (pending: {
        resolve: (decision: ToolApprovalDecision) => void;
        request: ToolApprovalRequest;
        signal?: AbortSignal;
      }) => void,
    ): void;
  }): void {
    this.approval.setApprovalBridge(bridge);
  }

  onNotify?: (message: string, severity?: "info" | "success" | "warning" | "error") => void;
  onNotifyInput?: (input: NotifyInput) => void;
  onCloseSurface?: (surfaceId: string) => void;

  get onOpenApprovalSurface(): (() => string) | undefined {
    return this.approval.onOpenApprovalSurface;
  }

  set onOpenApprovalSurface(handler: (() => string) | undefined) {
    this.approval.onOpenApprovalSurface = handler;
  }

  get approvalStore(): ApprovalStore | undefined {
    return this.approval.approvalStore;
  }

  set approvalStore(store: ApprovalStore | undefined) {
    this.approval.approvalStore = store;
  }

  private opIdCounter = 0;

  constructor(
    host: TuiHostFacade,
    store: TuiStore,
    settingsManager: SettingsManager,
    modelRegistry?: ModelRegistry,
    shutdownRuntime?: () => void,
  ) {
    this.host = host;
    this.store = store;
    this.modelRegistry = modelRegistry;
    this.settingsManager = settingsManager;
    this.shutdownRuntime = shutdownRuntime;
    this.hostd = new HostdActionAdapter(
      host,
      store,
      (event) => this.dispatch(event),
      (message, severity) => this.notify(message, severity),
    );
    this.runtimeConfig = new RuntimeConfigAdapter(host, this.hostd, settingsManager);
    this.approval = new ApprovalActionController(this.hostd, (event) => this.dispatch(event));

    this.session = new SessionActions({
      host: this.hostd.sessionHostPort(),
      dispatch: (event) => this.store.dispatch(event),
      closeSurface: (surfaceId) => {
        if (this.onCloseSurface) {
          this.onCloseSurface(surfaceId);
        } else {
          this.store.dispatch({ type: "surface_closed", surfaceId });
        }
      },
      notify: (notification) => {
        if (this.onNotifyInput) {
          this.onNotifyInput(notification);
        } else {
          this.onNotify?.(notification.message, notification.severity);
        }
      },
      nextOperationId: () => {
        this.opIdCounter++;
        return `op-${Date.now()}-${this.opIdCounter}`;
      },
      getCurrentRevision: () => {
        return this.store.state().input.revision;
      },
      isOperationActive: (operationId) => {
        return this.store.state().session.navigation.operationId === operationId;
      },
    });

    // Register persistent lifecycle callback on Host.
    // queue_update events flow through here whether triggered by the
    // run loop or by steer() / followUp() / dequeue().
    this.host.setLifecycleCallback((e: any) => {
      if (e.type === "queue_update") {
        this.dispatch({
          type: "queue_update",
          steerCount: e.steer_count,
          steerPreview: e.steer_preview,
          followUpCount: e.follow_up_count,
          followUpPreview: e.follow_up_preview,
        });
      }
    });
  }

  setHostdClient(client: HostdClient): void {
    this.hostd.setClient(client);
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

  async submitPrompt(text: string, _images?: ImageContent[]): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;

    if (this.hostd.enabled) {
      await this.hostd.submitPrompt(trimmed);
      return;
    }

    const ac = new AbortController();
    const state = this.getState();
    const promptSpan = startDebugSpan("tui.prompt", { agentId: state.currentAgentId });
    // Let Host decide: idle → stream, running → queue
    const streamOrNull = this.host.prompt(trimmed, "auto", state.currentAgentId, ac.signal);

    // Host queued the message (steer/followUp) — no stream to process
    if (!streamOrNull) {
      promptSpan.end({ outcome: "completed", status: "queued" });
      this.dispatch({ type: "user_submitted", text: trimmed });
      return;
    }

    this.abortController = ac;

    this.store.batchDispatch([
      { type: "user_submitted", text: trimmed },
      { type: "stream_started" },
    ]);

    try {
      const stream = streamOrNull;

      for await (const event of stream) {
        if (event.type === "text_delta") {
          this.dispatch({ type: "assistant_delta", delta: event.delta });
        } else if (event.type === "thinking_delta") {
          this.dispatch({ type: "thinking_delta", delta: event.delta });
        } else if (event.type === "tool_start") {
          this.dispatch({
            type: "tool_call_started",
            id: event.tool_call_id,
            name: event.tool_name,
            args: event.args,
          });
        } else if (event.type === "tool_end") {
          this.dispatch({
            type: "tool_call_ended",
            id: event.tool_call_id,
            name: event.tool_name,
            result: event.result,
            isError: event.is_error,
          });
        } else if (event.type === "queue_update") {
          this.dispatch({
            type: "queue_update",
            steerCount: event.steer_count,
            steerPreview: event.steer_preview,
            followUpCount: event.follow_up_count,
            followUpPreview: event.follow_up_preview,
          });
        }
        // message_start/message_end are informational only — handled by text_delta
      }

      const result = await stream.result();

      if (ac.signal.aborted || result.status === "aborted") {
        promptSpan.end({ outcome: "aborted", signalAborted: ac.signal.aborted });
        this.notify("Stream aborted", "warning");
        this.dispatch({
          type: "turn_finished",
          status: "aborted",
          transcript: this.getState().transcript as any,
        });
      } else {
        promptSpan.end({
          outcome: result.status === "error" ? "error" : "completed",
          status: result.status,
        });
        if (result.status === "error") {
          const errMsg = result.error ?? "Model run failed";
          this.notify(`Stream error: ${errMsg}`, "error");
          this.dispatch({
            type: "turn_failed",
            error: errMsg,
          });
        }
        // Rebuild canonical transcript from engine result
        this.dispatch({
          type: "turn_finished",
          status: result.status,
          transcript: result.messages,
          entries: await this.host.loadBranchEntries(),
        });

        // Update usage using computeCumulativeUsage
        const u = computeCumulativeUsage(result.messages);
        const updatedState = this.getState();
        this.dispatch({
          type: "usage_updated",
          inputTokens: updatedState.usage.inputTokens + u.input,
          outputTokens: updatedState.usage.outputTokens + u.output,
          cacheReadTokens: updatedState.usage.cacheReadTokens + u.cacheRead,
          cacheWriteTokens: updatedState.usage.cacheWriteTokens + u.cacheWrite,
          totalCost: updatedState.usage.totalCost + u.cost,
        });
      }
    } catch (err) {
      promptSpan.end({
        outcome: ac.signal.aborted ? "aborted" : "error",
        signalAborted: ac.signal.aborted,
      });
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
    } finally {
      if (this.abortController === ac) {
        this.abortController = null;
      }
      const currentStatus = this.getState().stream.status;
      if (currentStatus === "running" || currentStatus === "aborting") {
        this.dispatch({ type: "stream_settled" });
      }
    }
  }

  // ==========================================================================
  // Dequeue — clear all queues, return messages
  // ==========================================================================

  /**
   * Clear all queued messages and return them as a single string.
   * Returns null if no messages were queued.
   */
  dequeue(): string | null {
    const state = this.getState();
    const { steering, followUp, nextTurn } = this.host.dequeue(state.currentAgentId);
    const all = [...steering, ...followUp, ...nextTurn];
    if (all.length === 0) return null;
    return all.map((m) => m.text).join("\n\n");
  }

  // ==========================================================================
  // Follow-up — queue message as followUp (runs after current turn)
  // ==========================================================================

  /**
   * Submit text as a follow-up message.
   * If idle, acts like normal prompt. If running, queues as followUp.
   */
  followUp(text: string, images?: ImageContent[]): void {
    const trimmed = text.trim();
    if (!trimmed) return;

    const state = this.getState();
    const stream = this.host.prompt(trimmed, "followUp", state.currentAgentId);
    if (!stream) {
      // Queued as followUp
      this.dispatch({ type: "user_submitted", text: trimmed });
      return;
    }
    // Idle — start normal stream (same as submitPrompt path)
    this.submitPrompt(trimmed, images);
  }

  // ==========================================================================
  // Abort
  // ==========================================================================

  abortRun(): void {
    debugTrace({
      stage: "tui.abort.dispatched",
      signalAborted: this.abortController?.signal.aborted ?? false,
      status: this.getState().stream.status,
    });
    this.hostd.cancelTurn();
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

    this.runtimeConfig.applyModel(resolved.model, resolved.providerConfig);

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
    this.runtimeConfig.applyThinkingLevel(level);
    this.notify(`Thinking: ${level}`, "info");
    this.dispatch({ type: "thinking_level_changed", level });
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
