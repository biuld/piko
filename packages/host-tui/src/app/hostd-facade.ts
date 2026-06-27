// ============================================================================
// hostd-facade — TuiHostFacade backed by HostdClient (Rust hostd process)
//
// Replaces the old stub facade. All session operations go through the wire
// to hostd, which owns session storage, auth, models, and turn execution.
// ============================================================================

import type { HostdClient } from "../client/hostd-client.js";
import type {
  HostEvent,
  HostSessionSnapshot,
  SessionId,
  SessionSummary,
} from "../client/hostd-protocol.js";
import { createHostConfig } from "./host-config.js";

// ---- Lazy session snapshot ----

class SessionStore {
  private _sessionId: SessionId | null = null;
  private _snapshot: HostSessionSnapshot | null = null;
  private _sessions: SessionSummary[] = [];
  private snapshotWaiters = new Set<() => void>();
  private listWaiters = new Set<() => void>();

  get sessionId(): SessionId | null {
    return this._sessionId;
  }

  get snapshot(): HostSessionSnapshot | null {
    return this._snapshot;
  }

  get sessions(): SessionSummary[] {
    return [...this._sessions];
  }

  waitForSnapshot(sessionId?: SessionId, timeoutMs = 10_000): Promise<HostSessionSnapshot> {
    const current = this._snapshot;
    if (current && (!sessionId || current.session_id === sessionId)) {
      return Promise.resolve(current);
    }

    return this.waitForNextSnapshot(sessionId, timeoutMs);
  }

  waitForNextSnapshot(sessionId?: SessionId, timeoutMs = 10_000): Promise<HostSessionSnapshot> {
    return new Promise((resolve, reject) => {
      let done = false;
      const finish = () => {
        const snapshot = this._snapshot;
        if (!snapshot || (sessionId && snapshot.session_id !== sessionId)) return;
        if (done) return;
        done = true;
        clearTimeout(timer);
        this.snapshotWaiters.delete(finish);
        resolve(snapshot);
      };
      const timer = setTimeout(() => {
        if (done) return;
        done = true;
        this.snapshotWaiters.delete(finish);
        reject(new Error("timed out waiting for hostd session snapshot"));
      }, timeoutMs);
      this.snapshotWaiters.add(finish);
    });
  }

  waitForSessionList(timeoutMs = 10_000): Promise<SessionSummary[]> {
    return new Promise((resolve, reject) => {
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        clearTimeout(timer);
        this.listWaiters.delete(finish);
        resolve(this.sessions);
      };
      const timer = setTimeout(() => {
        if (done) return;
        done = true;
        this.listWaiters.delete(finish);
        reject(new Error("timed out waiting for hostd session list"));
      }, timeoutMs);
      this.listWaiters.add(finish);
    });
  }

  apply(event: HostEvent): void {
    if (
      event.type === "session_opened" ||
      event.type === "state_snapshot" ||
      event.type === "session_created"
    ) {
      let snapshot: HostSessionSnapshot;
      if (event.type === "session_created") {
        snapshot = {
          session_id: event.session_id,
          cwd: event.cwd,
          seq: 0,
          entries: [],
          current_leaf_id: null,
          active_turn: null,
          pending_approvals: [],
        };
      } else {
        snapshot = event.snapshot;
      }
      this._sessionId = snapshot.session_id;
      this._snapshot = snapshot;
      for (const waiter of [...this.snapshotWaiters]) {
        waiter();
      }
    } else if (event.type === "session_listed") {
      this._sessions = event.sessions;
      for (const waiter of [...this.listWaiters]) {
        waiter();
      }
    }
  }
}

// ---- Facade ----

export function createHostdFacade(
  client: HostdClient,
  options: { cwd?: string; session?: string | null; debugTracePath?: string },
): any {
  const cwd = options.cwd ?? process.cwd();
  const store = new SessionStore();
  let config: any = null;
  let thinkingLevel: string | undefined;

  // Subscribe to hostd events for session tracking
  client.onEvent((event) => {
    store.apply(event);
    if (event.type === "model_config_changed") {
      config = createHostConfig(
        { id: event.model_id, name: event.model_id, provider: event.provider } as any,
        {},
      );
    }
  });

  // Initialize in background
  const initComplete = (async () => {
    if (options.session && options.session !== "") {
      // Open existing session
      await client.send({
        type: "session_open",
        command_id: crypto.randomUUID(),
        session_id: options.session,
      });
    } else {
      // Create new session
      await client.send({
        type: "session_create",
        command_id: crypto.randomUUID(),
        cwd,
      });
    }
    await store.waitForSnapshot();
  })();

  return {
    get cwd() {
      return cwd;
    },
    get sessionId() {
      return store.sessionId ?? options.session ?? "";
    },
    sessionFile: options.session ?? "",
    teamMode: false,
    version: "hostd",
    debugTracePath: options.debugTracePath,

    // ---- Config ----

    getConfig: () => {
      if (!config) {
        config = createHostConfig({ id: "default", name: "default", provider: "default" } as any, {
          apiKey: "",
        });
      }
      return config;
    },
    setConfig: (next: any) => {
      config = next;
      // Push to hostd
      const model = next?.model;
      if (model?.id && model?.provider) {
        client
          .send({
            type: "config_set",
            command_id: crypto.randomUUID(),
            default_model: model.id,
            default_provider: model.provider,
          })
          .catch(() => {});
      }
    },
    getThinkingLevel: () => thinkingLevel,
    setThinkingLevel: (level: any) => {
      thinkingLevel = level;
      client
        .send({
          type: "config_set",
          command_id: crypto.randomUUID(),
          default_thinking_level: String(level ?? ""),
        })
        .catch(() => {});
    },
    setLifecycleCallback: () => {},

    // ---- Session ops ----

    restoreFromSession: async () => {
      await initComplete;
      const sid = store.sessionId;
      if (!sid) return;
      await client.resume(sid);
    },

    loadMessages: async () => [],
    loadBranchEntries: async () => store.snapshot?.entries ?? [],
    getSessionName: async () => store.snapshot?.name ?? null,
    setSessionName: async (name: string) => {
      await initComplete;
      const sid = store.sessionId;
      if (!sid) return;
      const pending = store.waitForNextSnapshot(sid);
      await client.send({
        type: "session_rename",
        command_id: crypto.randomUUID(),
        session_id: sid,
        name: name ?? "",
      });
      await pending;
    },

    newSession: async () => {
      const pending = store.waitForNextSnapshot();
      await client.send({
        type: "session_create",
        command_id: crypto.randomUUID(),
        cwd,
      });
      await pending;
    },

    cloneSession: async () => {
      await initComplete;
      const sid = store.sessionId;
      if (!sid) return;
      const pending = store.waitForNextSnapshot();
      await client.send({
        type: "session_fork",
        command_id: crypto.randomUUID(),
        session_id: sid,
      });
      await pending;
    },

    switchSession: async (sessionId: string) => {
      const pending = store.waitForSnapshot(sessionId);
      await client.send({
        type: "session_open",
        command_id: crypto.randomUUID(),
        session_id: sessionId,
      });
      await pending;
      return null;
    },

    navigateToEntry: async (entryId: string) => {
      await initComplete;
      const sid = store.sessionId;
      if (!sid) throw new Error("No active session");
      const pending = store.waitForNextSnapshot(sid);
      await client.send({
        type: "session_navigate",
        command_id: crypto.randomUUID(),
        session_id: sid,
        entry_id: entryId,
      });
      await pending;
      return {
        status: "navigated" as const,
        sessionId: sid,
        oldLeafId: null,
        newLeafId: entryId,
        selectedEntryId: entryId,
        branchEntries: [],
      };
    },

    forkSession: async (entryId?: string) => {
      await initComplete;
      const sid = store.sessionId;
      if (!sid) return {};
      const pending = store.waitForNextSnapshot();
      await client.send({
        type: "session_fork",
        command_id: crypto.randomUUID(),
        session_id: sid,
        entry_id: entryId,
      });
      await pending;
      return {};
    },

    importSession: async (path: string) => {
      const pending = store.waitForNextSnapshot();
      await client.send({
        type: "session_import",
        command_id: crypto.randomUUID(),
        path,
      });
      await pending;
    },

    renameSession: async (sessionId: string, name: string) => {
      const pending = store.waitForNextSnapshot(sessionId);
      await client.send({
        type: "session_rename",
        command_id: crypto.randomUUID(),
        session_id: sessionId,
        name,
      });
      await pending;
    },

    listSessions: async () => {
      const pending = store.waitForSessionList();
      await client.send({
        type: "session_list",
        command_id: crypto.randomUUID(),
      });
      return pending;
    },

    getLeafId: async () => store.snapshot?.current_leaf_id ?? undefined,
    getTreeEntries: async () => store.snapshot?.entries ?? [],
    getContextFiles: () => [],
    getActiveToolNames: () => [],
    getTotalToolCount: () => 0,
    getOrchestratorSnapshot: () => undefined,

    // ---- Turn execution ----

    prompt: () => null,
    dequeue: () => {},
    runSkill: async () => {},
    runPromptTemplate: async () => {},
    compact: async () => ({ message: "Compaction is handled by hostd" }),
    setSteeringMode: () => {},
    setFollowUpMode: () => {},

    // ---- Expose client for adapter ----

    _client: client,
    _store: store,
  };
}
