import type { SessionHostPort } from "../../actions/session-actions.js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import { type HostdClient, hostEventToTuiEvents } from "../../client/index.js";
import type { ToolApprovalDecision } from "../../shared/index.js";
import type { TuiEvent } from "../../state/events.js";
import type { TuiStore } from "./store.js";

type Notify = (message: string, severity?: "info" | "success" | "warning" | "error") => void;
type Dispatch = (event: TuiEvent) => void;

export class HostdActionAdapter {
  private client?: HostdClient;
  private sessionId?: string;
  private turnId?: string;

  constructor(
    private readonly host: TuiHostFacade,
    private readonly store: TuiStore,
    private readonly dispatch: Dispatch,
    private readonly notify: Notify,
  ) {}

  get enabled(): boolean {
    return this.client !== undefined;
  }

  get activeTurnId(): string | undefined {
    return this.turnId;
  }

  setClient(client: HostdClient): void {
    this.client = client;
    client.onEvent((event) => {
      if (event.type === "session_created") {
        this.sessionId = event.session_id;
      }
      if (event.type === "turn_started") {
        this.turnId = event.turn_id;
      } else if (
        event.type === "turn_completed" ||
        event.type === "turn_failed" ||
        event.type === "turn_cancelled"
      ) {
        if (this.turnId === event.turn_id) this.turnId = undefined;
      }

      const result = hostEventToTuiEvents(event);
      if (result === null) return;
      if (Array.isArray(result)) {
        for (const tuiEvent of result) this.dispatch(tuiEvent);
      } else {
        this.dispatch(result);
      }
    });

    // Session is already opened by the facade; just resume events
    const sessionId = this.currentSessionId(false);
    if (sessionId) {
      client.resume(sessionId).catch(() => {});
    }
  }

  sessionHostPort(): SessionHostPort {
    const adapter = this;
    return {
      navigateToEntry: async (entryId) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        const sid = this.currentSessionId();
        await this.send({
          type: "session_navigate",
          command_id: crypto.randomUUID(),
          session_id: sid,
          entry_id: entryId,
        });
        return {
          status: "navigated" as const,
          sessionId: sid,
          oldLeafId: null,
          newLeafId: entryId,
          selectedEntryId: entryId,
          branchEntries: [],
        };
      },
      forkSession: async (entryId) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_fork",
          command_id: crypto.randomUUID(),
          session_id: this.currentSessionId(),
          entry_id: entryId,
        });
        return {};
      },
      importSession: async (path) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_import",
          command_id: crypto.randomUUID(),
          path,
        });
      },
      renameSession: async (sessionId, name) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_rename",
          command_id: crypto.randomUUID(),
          session_id: sessionId,
          name,
        });
      },
      setSessionName: async (name) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_rename",
          command_id: crypto.randomUUID(),
          session_id: this.currentSessionId(),
          name: name ?? "",
        });
      },
      switchSession: async (specifier) => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_open",
          command_id: crypto.randomUUID(),
          session_id: specifier,
        });
        return null;
      },
      newSession: async () => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_create",
          command_id: crypto.randomUUID(),
          cwd: this.store.state().session.cwd,
        });
      },
      cloneSession: async () => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        await this.send({
          type: "session_fork",
          command_id: crypto.randomUUID(),
          session_id: this.currentSessionId(),
        });
      },
      restoreFromSession: () => {
        if (!this.enabled) throw new Error("hostd client is not configured");
        const sid = this.currentSessionId(false);
        return sid ? this.client!.resume(sid) : Promise.resolve();
      },
      get sessionId() {
        return adapter.currentSessionId(false) ?? adapter.host.sessionId;
      },
      getConfig: () => this.host.getConfig(),
      getThinkingLevel: () => this.host.getThinkingLevel(),
    };
  }

  async submitPrompt(text: string): Promise<void> {
    const sessionId = await this.ensureSession();
    const streamStatus = this.store.state().stream.status;
    const shouldQueue = streamStatus === "running" || streamStatus === "aborting";
    this.dispatch({ type: "user_submitted", text });
    if (shouldQueue) {
      await this.send({
        type: "queue_follow_up",
        command_id: crypto.randomUUID(),
        session_id: sessionId,
        message: text,
      });
      return;
    }
    await this.send({
      type: "turn_submit",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
      text,
    });
  }

  async queueFollowUp(text: string): Promise<void> {
    const sessionId = await this.ensureSession();
    this.dispatch({ type: "user_submitted", text });
    await this.send({
      type: "queue_follow_up",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
      message: text,
    });
  }

  cancelTurn(): void {
    if (!this.enabled || !this.turnId) return;
    const sessionId = this.currentSessionId(false);
    if (!sessionId) return;
    void this.send({
      type: "turn_cancel",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
      turn_id: this.turnId,
    }).catch((error) => this.notifyError(error));
  }

  respondApproval(approvalId: string, decision: ToolApprovalDecision): void {
    if (!this.enabled) return;
    const sessionId = this.currentSessionId(false);
    if (!sessionId) {
      this.notify("No active hostd session for approval", "error");
      return;
    }
    void this.send({
      type: "approval_respond",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
      approval_id: approvalId,
      decision:
        decision === "decline"
          ? "decline"
          : decision === "accept_session"
            ? "accept_session"
            : decision === "accept_workspace"
              ? "accept_workspace"
              : "accept",
    }).catch((error) => this.notifyError(error));
  }

  setModel(provider: string, model: string): void {
    if (!this.enabled) return;
    void this.send({
      type: "config_set",
      command_id: crypto.randomUUID(),
      default_provider: provider,
      default_model: model,
    }).catch((error) => this.notifyError(error));
  }

  setThinkingLevel(level: string): void {
    if (!this.enabled) return;
    void this.send({
      type: "config_set",
      command_id: crypto.randomUUID(),
      default_thinking_level: level,
    }).catch((error) => this.notifyError(error));
  }

  compactSession(sessionId: string): void {
    if (!this.enabled) return;
    void this.send({
      type: "session_compact",
      command_id: crypto.randomUUID(),
      session_id: sessionId,
    }).catch((error) => this.notifyError(error));
  }

  startAuthLogin(provider: string): void {
    if (!this.enabled) {
      this.notify("hostd client is not configured for auth login", "error");
      return;
    }
    void this.send({
      type: "auth_login_start",
      command_id: crypto.randomUUID(),
      provider,
    }).catch((error) => this.notifyError(error));
  }

  private async send(command: Parameters<HostdClient["send"]>[0]): ReturnType<HostdClient["send"]> {
    if (!this.client) throw new Error("hostd client is not configured");
    return this.client.send(command);
  }

  private currentSessionId(required?: true): string;
  private currentSessionId(required: false): string | undefined;
  private currentSessionId(required = true): string | undefined {
    const sessionId = this.sessionId ?? this.store.state().session.sessionId ?? this.host.sessionId;
    if (!sessionId && required) {
      throw new Error("No active hostd session");
    }
    return sessionId;
  }

  private async ensureSession(): Promise<string> {
    if (this.sessionId) return this.sessionId;
    const stateSessionId = this.store.state().session.sessionId;
    if (stateSessionId) return stateSessionId;
    if (!this.client) throw new Error("hostd client is not configured");

    return await new Promise<string>((resolve, reject) => {
      let unsubscribe: (() => void) | undefined;
      const timer = setTimeout(() => {
        unsubscribe?.();
        reject(new Error("hostd did not create a session"));
      }, 10_000);

      const client = this.client;
      if (!client) {
        clearTimeout(timer);
        reject(new Error("hostd client is not configured"));
        return;
      }

      unsubscribe = client.onEvent((event) => {
        if (event.type !== "session_created") return;
        clearTimeout(timer);
        unsubscribe?.();
        this.sessionId = event.session_id;
        resolve(event.session_id);
      });

      client
        .send({
          type: "session_create",
          command_id: crypto.randomUUID(),
          cwd: this.store.state().session.cwd,
        })
        .catch((error) => {
          clearTimeout(timer);
          unsubscribe?.();
          reject(error);
        });
    });
  }

  private notifyError(error: unknown): void {
    this.notify(error instanceof Error ? error.message : String(error), "error");
  }
}
