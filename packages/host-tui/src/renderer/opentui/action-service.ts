// ============================================================================
// ActionService — stable service object holding host, abort controller,
// model registry, and all side-effect action methods.
//
// This replaces the transient ActionContext that was recreated on every
// Solid render, which caused the abort controller to be lost.
// ============================================================================

import {
  computeCumulativeUsage,
  createHostConfig,
  type ModelRegistry,
  type PikoHost,
  type SettingsManager,
} from "piko-host-runtime";
import type {
  ImageContent,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "piko-orchestrator-protocol";
import { debugTrace, startDebugSpan } from "piko-orchestrator-protocol";
import { SessionActions } from "../../actions/session-actions.js";
import type { ApprovalStore } from "../../approval-store.js";
import type { NotifyInput } from "../../notifications/types.js";
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
  readonly settingsManager: SettingsManager;
  readonly session: SessionActions;

  /** Current abort controller for the running stream. Stable across renders. */
  abortController: AbortController | null = null;

  /** Cleanup callback set by the renderer entry point. Called before process exit. */
  private readonly shutdownRuntime?: () => void;

  // ---- Approval gateway ----
  /** Pending approval requests keyed by internal tool entity identity. */
  private pendingApprovals = new Map<
    string,
    {
      resolve: (decision: ToolApprovalDecision) => void;
      reject: (err: Error) => void;
      request: ToolApprovalRequest;
    }
  >();

  /**
   * Approval handler for the Orchestrator's ApprovalGateway.
   * Stores the request and returns a Promise that resolves when the user decides.
   */
  readonly approvalHandler = (
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision> => {
    const callId = request.callId;
    const entityId = request.toolEntityId || callId;

    // If already aborted, reject immediately
    if (signal?.aborted) {
      return Promise.resolve("decline");
    }

    // Check if this tool call is already approved at session/workspace/permanent scope.
    // Fingerprint-based: e.g. "bash:git" matches all git commands, unknown commands
    // fall back to full-string matching.
    if (this.approvalStore?.isApproved(request.toolName, request.toolArgs)) {
      return Promise.resolve("accept");
    }

    return new Promise<ToolApprovalDecision>((resolve, reject) => {
      const entry = { resolve, reject, request };
      this.pendingApprovals.set(entityId, entry);

      // Dispatch approval_needed event to show UI
      this.dispatch({
        type: "approval_needed",
        toolEntityId: request.toolEntityId,
        callId: request.callId,
        toolName: request.toolName,
        toolArgs: request.toolArgs,
      });

      // Open the approval surface (like /model). If a surface is already
      // open, close it first so only the newest pending request is visible.
      this.onOpenApprovalSurface?.();

      // Listen for abort signal
      if (signal) {
        const onAbort = () => {
          this.pendingApprovals.delete(entityId);
          this.dispatch({
            type: "approval_resolved",
            toolEntityId: entityId,
            callId,
            decision: "decline",
          });
          resolve("decline");
        };
        signal.addEventListener("abort", onAbort, { once: true });

        // Clean up listener if promise resolves normally
        const origResolve = resolve;
        const origReject = reject;
        const wrapped = (decision: ToolApprovalDecision) => {
          signal.removeEventListener("abort", onAbort);
          origResolve(decision);
        };
        const wrappedReject = (err: Error) => {
          signal.removeEventListener("abort", onAbort);
          origReject(err);
        };
        this.pendingApprovals.set(entityId, { resolve: wrapped, reject: wrappedReject, request });
      }
    });
  };

  /**
   * Resolve a pending approval by callId. Called from the approval surface controller.
   * If the decision includes a scope, stores the approval so future calls of the
   * same tool are auto-accepted.
   */
  resolveApproval(toolEntityId: string, decision: ToolApprovalDecision): void {
    const entry = this.pendingApprovals.get(toolEntityId);
    if (!entry) return;
    this.pendingApprovals.delete(toolEntityId);
    const callId = entry.request.callId;

    // Store scoped approvals for future auto-accept
    if (decision === "accept_session") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "session");
    } else if (decision === "accept_workspace") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "workspace");
    } else if (decision === "accept_permanent") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "permanent");
    }

    this.dispatch({ type: "approval_resolved", toolEntityId, callId, decision });
    debugTrace({
      stage: "approval.tui.resolved",
      taskId: entry.request.taskId,
      agentId: entry.request.agentId,
      toolCallId: callId,
      toolName: entry.request.toolName,
      status: decision,
    });
    entry.resolve(decision);

    // Close the approval surface. If no more pending, the surface disappears.
    if (this.pendingApprovals.size === 0) {
      // The surface controller handles close on confirm/decline, but also
      // close proactively when the last pending entry is resolved.
    }
  }

  /**
   * Set the pre-created approval bridge from opentui-runtime.
   * Registers a listener that forwards all pending approvals (past and future)
   * into this.pendingApprovals and dispatches approval_needed events.
   */
  setApprovalBridge(bridge: {
    onPending(
      listener: (pending: {
        resolve: (decision: ToolApprovalDecision) => void;
        request: ToolApprovalRequest;
        signal?: AbortSignal;
      }) => void,
    ): void;
  }): void {
    bridge.onPending((pending) => {
      const callId = pending.request.callId;
      const entityId = pending.request.toolEntityId || callId;
      const onAbort = () => {
        this.pendingApprovals.delete(entityId);
        this.dispatch({
          type: "approval_resolved",
          toolEntityId: entityId,
          callId,
          decision: "decline",
        });
        debugTrace({
          stage: "approval.tui.resolved",
          taskId: pending.request.taskId,
          agentId: pending.request.agentId,
          toolCallId: callId,
          toolName: pending.request.toolName,
          outcome: "aborted",
        });
      };
      const resolve = (decision: ToolApprovalDecision) => {
        pending.signal?.removeEventListener("abort", onAbort);
        pending.resolve(decision);
      };
      this.pendingApprovals.set(entityId, {
        resolve,
        reject: () => {},
        request: pending.request,
      });
      pending.signal?.addEventListener("abort", onAbort, { once: true });
      debugTrace({
        stage: "approval.tui.received",
        taskId: pending.request.taskId,
        agentId: pending.request.agentId,
        toolCallId: callId,
        toolName: pending.request.toolName,
      });
      this.dispatch({
        type: "approval_needed",
        toolEntityId: pending.request.toolEntityId,
        callId,
        toolName: pending.request.toolName,
        toolArgs: pending.request.toolArgs,
      });
      this.onOpenApprovalSurface?.();
    });
  }

  onNotify?: (message: string, severity?: "info" | "success" | "warning" | "error") => void;
  onNotifyInput?: (input: NotifyInput) => void;
  onCloseSurface?: (surfaceId: string) => void;
  /** Open the tool-approval surface. Called from approvalHandler. */
  onOpenApprovalSurface?: () => string;
  /** Approval store for scoped (session/workspace/permanent) approvals. */
  approvalStore?: ApprovalStore;

  private opIdCounter = 0;

  constructor(
    host: PikoHost,
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

    this.session = new SessionActions({
      host: {
        navigateToEntry: (entryId) => this.host.navigateToEntry(entryId),
        forkSession: (entryId) => this.host.forkSession(entryId),
        importSession: (path) => this.host.importSession(path).then(() => {}),
        renameSession: (sessionId, name) => this.host.renameSession(sessionId, name).then(() => {}),
        setSessionName: (name) => this.host.setSessionName(name),
        switchSession: (specifier) => this.host.switchSession(specifier),
        newSession: () => this.host.newSession().then(() => {}),
        cloneSession: () => this.host.cloneSession().then(() => {}),
        restoreFromSession: () => this.host.restoreFromSession(),
        loadBranchEntries: () => this.host.loadBranchEntries(),
        getSessionName: () => this.host.getSessionName().then((n) => n ?? null),
        get sessionId() {
          return host.sessionId;
        },
        loadMessages: () => this.host.loadMessages(),
        getConfig: () => this.host.getConfig(),
        getThinkingLevel: () => this.host.getThinkingLevel(),
      },
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
    this.host.setLifecycleCallback((e) => {
      if (e.type === "queue_update") {
        this.dispatch({
          type: "queue_update",
          agentId: e.agentId,
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

  async submitPrompt(text: string, _images?: ImageContent[]): Promise<void> {
    const trimmed = text.trim();
    if (!trimmed) return;

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
        if (event.type === "message_start") {
          this.dispatch({
            type: "message_start",
            message: event.message,
            runId: event.runId,
            eventSeq: event.eventSeq,
            turnIndex: event.turnIndex,
            messageIndex: event.messageIndex,
          });
        } else if (event.type === "message_update") {
          this.dispatch({
            type: "message_update",
            message: event.message,
            assistantEvent: event.assistantEvent,
            runId: event.runId,
            eventSeq: event.eventSeq,
            turnIndex: event.turnIndex,
            messageIndex: event.messageIndex,
          });
        } else if (event.type === "message_end") {
          this.dispatch({
            type: "message_end",
            message: event.message,
            runId: event.runId,
            eventSeq: event.eventSeq,
            turnIndex: event.turnIndex,
            messageIndex: event.messageIndex,
          });
        } else if (event.type === "tool_execution_start") {
          this.dispatch({
            type: "tool_call_started",
            entityId: event.toolEntityId,
            id: event.toolCallId,
            name: event.toolName,
            args: event.args,
            runId: event.runId,
            eventSeq: event.eventSeq,
            turnIndex: event.turnIndex,
            parentMessageId: (event as any).parentMessageId ?? "",
            contentIndex: (event as any).contentIndex ?? 0,
            toolCallIndex: (event as any).toolCallIndex ?? 0,
          });
        } else if (event.type === "tool_execution_end") {
          this.dispatch({
            type: "tool_call_ended",
            entityId: event.toolEntityId,
            id: event.toolCallId,
            name: event.toolName,
            result: event.result,
            isError: event.isError,
            runId: event.runId,
            eventSeq: event.eventSeq,
            turnIndex: event.turnIndex,
            parentMessageId: (event as any).parentMessageId ?? "",
            contentIndex: (event as any).contentIndex ?? 0,
            toolCallIndex: (event as any).toolCallIndex ?? 0,
          });
        } else if (event.type === "queue_update") {
          this.dispatch({
            type: "queue_update",
            steerCount: event.steerCount,
            steerPreview: event.steerPreview,
            followUpCount: event.followUpCount,
            followUpPreview: event.followUpPreview,
          });
        }
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
        // Rebuild canonical transcript from engine result
        this.dispatch({
          type: "turn_finished",
          status: result.status,
          transcript: result.messages,
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
      createHostConfig(resolved.model, resolved.providerConfig, currentConfig.settings),
    );

    this.settingsManager.setDefaultModelAndProvider(resolved.model.provider, resolved.model.id);

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
    this.settingsManager.setDefaultThinkingLevel(level as any);
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
