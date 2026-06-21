import { describe, expect, it } from "bun:test";
import type { TreeNavigationResult } from "piko-host-runtime";
import { SessionActions } from "../src/actions/session-actions.js";
import type { NotifyInput } from "../src/notifications/types.js";
import type { TuiEvent } from "../src/state/events.js";

describe("SessionActions", () => {
  it("dispatches started then succeeded, closes surface, and notifies on success", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      navigateToEntry: async (entryId: string): Promise<TreeNavigationResult> => {
        return {
          status: "navigated",
          sessionId: "session-123",
          oldLeafId: "old-leaf",
          newLeafId: "new-leaf",
          selectedEntryId: entryId,
          editorContent: "Original user text",
          branchEntries: [
            {
              type: "message",
              id: "msg-1",
              parentId: null,
              timestamp: new Date().toISOString(),
              message: { role: "user", content: "Original user text", timestamp: Date.now() },
            },
          ],
        };
      },
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.navigateTree("msg-1", "surface-789");

    expect(events).toHaveLength(2);
    expect(events[0]).toEqual({
      type: "tree_navigation_started",
      operationId: "op-456",
      entryId: "msg-1",
    });
    expect(events[1].type).toBe("tree_navigation_succeeded");

    const succEvent = events[1] as any;
    expect(succEvent.operationId).toBe("op-456");
    expect(succEvent.result.status).toBe("navigated");
    expect(succEvent.result.editorDraft).toEqual({
      text: "Original user text",
      revision: 11,
      source: {
        kind: "session_tree",
        sessionId: "session-123",
        entryId: "msg-1",
      },
    });

    expect(closedSurface).toBe("surface-789");
    expect(notification).toEqual({
      message: "Navigated to entry",
      severity: "success",
      source: "session",
    });
  });

  it("notifies differently on already_current", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      navigateToEntry: async (entryId: string): Promise<TreeNavigationResult> => {
        return {
          status: "already_current",
          sessionId: "session-123",
          oldLeafId: "old-leaf",
          newLeafId: "old-leaf",
          selectedEntryId: entryId,
          branchEntries: [],
        };
      },
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.navigateTree("msg-1", "surface-789");

    expect(events).toHaveLength(2);
    expect(events[0].type).toBe("tree_navigation_started");
    expect(events[1].type).toBe("tree_navigation_succeeded");
    expect(closedSurface).toBe("surface-789");
    expect(notification).toEqual({
      message: "Already at this point",
      severity: "info",
      source: "session",
    });
  });

  it("dispatches started then failed and notifies on host failure", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      navigateToEntry: async () => {
        throw new Error("Something went wrong");
      },
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.navigateTree("msg-1", "surface-789");

    expect(events).toHaveLength(2);
    expect(events[0].type).toBe("tree_navigation_started");
    expect(events[1]).toEqual({
      type: "tree_navigation_failed",
      operationId: "op-456",
      error: "Something went wrong",
    });
    expect(closedSurface).toBeUndefined(); // Keep open on failure
    expect(notification).toEqual({
      message: "Navigation failed: Something went wrong",
      severity: "error",
      source: "session",
    });
  });

  it("forkSession dispatches session_resumed, closes surface, notifies and replaces draft if selectedText exists", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "forked-session-123",
      forkSession: async (_entryId: string) => {
        return { selectedText: "Forked user text" };
      },
      getSessionName: async () => "Forked Session",
      loadBranchEntries: async () => [
        {
          type: "message",
          id: "msg-1",
          parentId: null,
          timestamp: new Date().toISOString(),
          message: { role: "user", content: "Forked user text", timestamp: Date.now() },
        },
      ],
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.forkSession("msg-1", "surface-789");

    expect(closedSurface).toBe("surface-789");
    expect(notification).toEqual({
      message: "Forked to new session",
      severity: "success",
      source: "session",
    });

    // Check dispatched events: session_resumed and editor_draft_replaced
    expect(events).toHaveLength(2);
    expect(events[0]).toEqual({
      type: "session_resumed",
      sessionId: "forked-session-123",
      sessionName: "Forked Session",
      transcript: [
        {
          id: "msg-1",
          role: "user",
          text: "Forked user text",
        },
      ],
    });
    expect(events[1]).toEqual({
      type: "editor_draft_replaced",
      text: "Forked user text",
    });
  });

  it("importSession dispatches session_resumed, closes surface and notifies", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "imported-session-456",
      importSession: async (_path: string) => {},
      getSessionName: async () => "Imported Session",
      loadBranchEntries: async () => [],
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.importSession("/some/path.jsonl", "surface-789");

    expect(closedSurface).toBe("surface-789");
    expect(notification).toEqual({
      message: "Session imported",
      severity: "success",
      source: "session",
    });
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("session_resumed");
  });

  it("renameSession renames current session and dispatches session_info_updated", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "session-123",
      setSessionName: async (_name: string) => {},
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.renameSession("New Name", undefined, "surface-789");

    expect(closedSurface).toBe("surface-789");
    expect(notification).toEqual({
      message: 'Session renamed to "New Name"',
      severity: "success",
      source: "session",
    });
    expect(events).toHaveLength(1);
    expect(events[0]).toEqual({
      type: "session_info_updated",
      sessionId: "session-123",
      sessionName: "New Name",
    });
  });

  it("switchSession updates model, thinking level, resumes session and closes surface", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "switched-session-789",
      switchSession: async (_specifier: string) => ({}),
      restoreFromSession: async () => {},
      getConfig: () => ({
        model: { id: "model-1", provider: "provider-1" },
        provider: { api: "api-1" },
      }),
      getThinkingLevel: () => "high",
      loadMessages: async () => [{}, {}],
      getSessionName: async () => "Switched Session",
      loadBranchEntries: async () => [],
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.switchSession("some-specifier", "surface-789");

    expect(closedSurface).toBe("surface-789");
    expect(notification?.severity).toBe("success");
    expect(events).toHaveLength(3);
    expect(events[0]).toEqual({
      type: "model_changed",
      model: { id: "model-1", provider: "provider-1" } as any,
      providerConfig: { api: "api-1" } as any,
    });
    expect(events[1]).toEqual({
      type: "thinking_level_changed",
      level: "high",
    });
    expect(events[2].type).toBe("session_resumed");
  });

  it("newSession starts new session and notifies", async () => {
    const events: TuiEvent[] = [];
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "new-session-id",
      newSession: async () => {},
      getSessionName: async () => null,
      loadBranchEntries: async () => [],
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: () => {},
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.newSession();

    expect(notification).toEqual({
      message: "New session started",
      severity: "success",
      source: "session",
    });
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("session_resumed");
  });

  it("cloneSession clones current session and notifies", async () => {
    const events: TuiEvent[] = [];
    let notification: NotifyInput | undefined;

    const hostMock = {
      sessionId: "cloned-session-id",
      cloneSession: async () => {},
      getSessionName: async () => null,
      loadBranchEntries: async () => [],
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => events.push(e),
      closeSurface: () => {},
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: () => true,
    });

    await actions.cloneSession();

    expect(notification).toEqual({
      message: "Session cloned",
      severity: "success",
      source: "session",
    });
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("session_resumed");
  });

  it("ignores tree navigation success if the operation is invalidated by session switch", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    let currentStoreOpId: string | undefined = "op-456";

    const hostMock = {
      navigateToEntry: async (entryId: string): Promise<TreeNavigationResult> => {
        // Simulate asynchronous operation
        await new Promise((resolve) => setTimeout(resolve, 5));
        return {
          status: "navigated",
          sessionId: "session-123",
          oldLeafId: "old-leaf",
          newLeafId: "new-leaf",
          selectedEntryId: entryId,
          branchEntries: [],
        };
      },
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => {
        events.push(e);
        if (e.type === "tree_navigation_started") {
          currentStoreOpId = e.operationId;
        }
      },
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: (opId) => currentStoreOpId === opId,
    });

    const navPromise = actions.navigateTree("msg-1", "surface-789");

    // Before navigation completes, simulate session resume / switch which clears the operation ID
    currentStoreOpId = undefined;

    await navPromise;

    // Success event should not be dispatched
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("tree_navigation_started");

    // Side effects must be skipped
    expect(closedSurface).toBeUndefined();
    expect(notification).toBeUndefined();
  });

  it("ignores tree navigation failure if the operation is invalidated by session switch", async () => {
    const events: TuiEvent[] = [];
    let closedSurface: string | undefined;
    let notification: NotifyInput | undefined;

    let currentStoreOpId: string | undefined = "op-456";

    const hostMock = {
      navigateToEntry: async () => {
        // Simulate asynchronous operation
        await new Promise((resolve) => setTimeout(resolve, 5));
        throw new Error("Stale navigation failed");
      },
    };

    const actions = new SessionActions({
      host: hostMock as any,
      dispatch: (e) => {
        events.push(e);
        if (e.type === "tree_navigation_started") {
          currentStoreOpId = e.operationId;
        }
      },
      closeSurface: (id) => {
        closedSurface = id;
      },
      notify: (n) => {
        notification = n;
      },
      nextOperationId: () => "op-456",
      getCurrentRevision: () => 10,
      isOperationActive: (opId) => currentStoreOpId === opId,
    });

    const navPromise = actions.navigateTree("msg-1", "surface-789");

    // Clear operation ID before it completes
    currentStoreOpId = undefined;

    await navPromise;

    // Failure event should not be dispatched
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe("tree_navigation_started");

    expect(closedSurface).toBeUndefined();
    expect(notification).toBeUndefined();
  });
});
