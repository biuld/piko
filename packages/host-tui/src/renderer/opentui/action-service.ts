// ============================================================================
// ActionService — stable service object holding host, abort controller,
// model registry, and all side-effect action methods.
//
// This replaces the transient ActionContext that was recreated on every
// Solid render, which caused the abort controller to be lost.
// ============================================================================

import { SessionActions } from "../../actions/session-actions.js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import type { TuiPreferences } from "../../app/tui-preferences.js";
import type { ApprovalStore } from "../../approval-store.js";
import type { HostdClient } from "../../client/index.js";
import type { NotifyInput } from "../../notifications/types.js";
import type {
  ImageContent,
  ToolApprovalDecision,
  ToolApprovalRequest,
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
  readonly preferences: TuiPreferences;
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
    preferences: TuiPreferences,
    shutdownRuntime?: () => void,
  ) {
    this.host = host;
    this.store = store;
    this.preferences = preferences;
    this.shutdownRuntime = shutdownRuntime;
    this.hostd = new HostdActionAdapter(
      host,
      store,
      (event) => this.dispatch(event),
      (message, severity) => this.notify(message, severity),
    );
    this.runtimeConfig = new RuntimeConfigAdapter(host, this.hostd);
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

    // Queue events flow through HostdClient.onEvent → hostd-action-adapter.
    // No lifecycle callback needed — hostd owns the event stream.
  }

  setHostdClient(client: HostdClient): void {
    this.hostd.setClient(client);
  }

  private notify(message: string, severity?: "info" | "success" | "warning" | "error"): void {
    this.onNotify?.(message, severity);
  }

  dispatch(event: TuiEvent): void {
    if (event.type === "auth_login_device_code") {
      this.notify(`Open ${event.verification_uri} and enter ${event.user_code}`, "info");
    } else if (event.type === "auth_login_success") {
      this.notify(`Logged in to ${event.provider}`, "success");
    } else if (event.type === "auth_login_failed") {
      this.notify(`Login failed for ${event.provider}: ${event.error}`, "error");
    } else if (event.type === "auth_logged_out") {
      this.notify(`Logged out from ${event.provider}`, "success");
    }
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
    // All prompt submission goes through hostd — no local fallback.
    await this.hostd.submitPrompt(trimmed);
  }

  // ==========================================================================
  // Dequeue — clear all queues, return messages
  // ==========================================================================

  /**
   * Dequeue is handled entirely by hostd (queue drain in turn completion).
   * This always returns null — the TUI no longer manages prompt queues locally.
   */
  dequeue(): string | null {
    return null;
  }

  // ==========================================================================
  // Follow-up — queue message as followUp (runs after current turn)
  // ==========================================================================

  /**
   * Submit text as a follow-up message.
   * If idle, acts like normal prompt. If running, queues as followUp.
   */
  followUp(text: string, _images?: ImageContent[]): void {
    const trimmed = text.trim();
    if (!trimmed) return;
    void this.hostd.queueFollowUp(trimmed).catch((error) => {
      this.notify(error instanceof Error ? error.message : String(error), "error");
    });
  }

  // ==========================================================================
  // Abort
  // ==========================================================================

  abortRun(): void {
    this.hostd.cancelTurn();
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    this.dispatch({ type: "aborted" });
  }

  // ==========================================================================
  // Compaction
  // ==========================================================================

  /** Manually trigger session compaction via hostd. */
  compactSession(): void {
    const sessionId = this.store.state().session.sessionId ?? this.host.sessionId;
    if (!sessionId) {
      this.notify("No active session to compact", "warning");
      return;
    }
    this.hostd.compactSession(sessionId);
    this.notify("Compaction triggered", "info");
  }

  // ==========================================================================
  // Model switching
  // ==========================================================================

  /** Switch model — sends config_set to hostd. Model list comes from hostd catalog. */
  switchModel(modelId: string, providerName: string): void {
    // Push to hostd — hostd owns auth + model resolution
    this.hostd.setModel(providerName, modelId);

    // Update local display immediately for responsiveness
    const state = this.getState();
    const modelEntry = state.model.modelCatalog
      ?.flatMap((p) => p.models.map((m) => ({ ...m, provider: p.provider })))
      .find((m) => m.id === modelId && m.provider === providerName);

    if (modelEntry) {
      this.notify(`Model: ${modelEntry.id}`, "success");
      this.dispatch({
        type: "model_changed",
        model: modelEntry as any,
        providerConfig: state.model.providerConfig,
      });
    } else {
      this.notify(`Model: ${providerName}/${modelId}`, "info");
    }
  }

  /**
   * Change thinking level.
   */
  setThinkingLevel(level: string): void {
    this.runtimeConfig.applyThinkingLevel(level);
    this.notify(`Thinking: ${level}`, "info");
    this.dispatch({ type: "thinking_level_changed", level });
  }

  startAuthOAuth(provider: string): void {
    this.hostd.startAuthOAuth(provider);
  }

  setApiKey(provider: string, apiKey: string): void {
    this.hostd.setApiKey(provider, apiKey);
  }

  logout(provider: string): void {
    this.hostd.logout(provider);
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
