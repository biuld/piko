import { afterEach, describe, expect, it } from "bun:test";
import { testRender } from "@opentui/solid";
import type { SessionTreeEntry } from "piko-session";
import { createComponent } from "solid-js";
import { TreeSelector } from "../src/renderer/opentui/select/TreeSelector.js";

const renderers: Array<{ destroy(): void }> = [];

afterEach(() => {
  for (const renderer of renderers.splice(0)) renderer.destroy();
});

function messageEntry(
  id: string,
  parentId: string | null,
  role: "user" | "assistant",
  text: string,
): SessionTreeEntry {
  return {
    type: "message",
    id,
    parentId,
    timestamp: new Date(0).toISOString(),
    message: {
      role,
      content: role === "user" ? text : [{ type: "text", text }],
      timestamp: 0,
    },
  } as unknown as SessionTreeEntry;
}

interface HarnessOptions {
  entries?: SessionTreeEntry[];
  leafId?: string | null;
  editorText?: string;
  navigate?: (entryId: string) => Promise<{ editorText?: string }>;
  branchEntries?: SessionTreeEntry[];
}

async function renderHarness(options: HarnessOptions = {}) {
  const entries = options.entries ?? [];
  let surfaceController:
    | {
        handleKey(event: { name: string }): { type: string };
        onConfirm?(): void;
      }
    | undefined;
  let closed = false;
  let restoredEditorText: string | undefined;
  const navigatedEntryIds: string[] = [];
  const dispatched: unknown[] = [];
  const notifications: Array<{ message: string; severity?: string }> = [];

  const controller = {
    setSurfaceController(_id: string, value: typeof surfaceController | null) {
      surfaceController = value ?? undefined;
    },
    getEditorText: () => options.editorText ?? "",
    setEditorText(text: string) {
      restoredEditorText = text;
    },
    notifications: {
      notify(notification: { message: string; severity?: string }) {
        notifications.push(notification);
      },
    },
  };
  const actionSvc = {
    getState: () => ({ layout: { viewport: { width: 80, height: 24 } } }),
    dispatch(event: unknown) {
      dispatched.push(event);
    },
  };
  const host = {
    sessionId: "session-1",
    getLeafId: () => options.leafId ?? null,
    getTreeEntries: async () => entries,
    navigateToEntry: async (entryId: string) => {
      navigatedEntryIds.push(entryId);
      return options.navigate?.(entryId) ?? { editorText: "Edit this prompt" };
    },
    loadBranchEntries: async () => options.branchEntries ?? [],
    getSessionName: async () => undefined,
  };

  const setup = await testRender(
    () =>
      createComponent(TreeSelector, {
        actionSvc: actionSvc as never,
        controller: controller as never,
        host: host as never,
        surfaceId: "tree-surface",
        availableWidth: 80,
        availableHeight: 20,
        onClose: () => {
          closed = true;
        },
      }),
    { width: 80, height: 24 },
  );
  renderers.push(setup.renderer);
  await setup.waitFor(() => surfaceController !== undefined);
  await setup.flush();

  return {
    setup,
    controller: () => surfaceController!,
    closed: () => closed,
    restoredEditorText: () => restoredEditorText,
    navigatedEntryIds,
    dispatched,
    notifications,
  };
}

async function confirm(harness: Awaited<ReturnType<typeof renderHarness>>): Promise<void> {
  expect(harness.controller().handleKey({ name: "enter" }).type).toBe("confirm");
  harness.controller().onConfirm?.();
}

describe("TreeSelector navigation", () => {
  it("navigates a root user entry and dispatches the truncated branch", async () => {
    const user = messageEntry("user-1", null, "user", "Edit this prompt");
    const assistant = messageEntry("assistant-1", user.id, "assistant", "Old reply");
    const harness = await renderHarness({ entries: [user, assistant], leafId: assistant.id });

    await confirm(harness);
    await harness.setup.waitFor(() => harness.dispatched.length === 1);

    expect(harness.closed()).toBe(true);
    expect(harness.navigatedEntryIds).toEqual([user.id]);
    expect(harness.dispatched).toEqual([
      {
        type: "session_resumed",
        sessionId: "session-1",
        sessionName: undefined,
        transcript: [],
      },
    ]);
    expect(harness.restoredEditorText()).toBe("Edit this prompt");
  });

  it("does nothing when the tree has no selectable user entry", async () => {
    const assistant = messageEntry("assistant-1", null, "assistant", "Only reply");
    const harness = await renderHarness({ entries: [assistant], leafId: assistant.id });

    await confirm(harness);
    await harness.setup.flush();

    expect(harness.closed()).toBe(false);
    expect(harness.navigatedEntryIds).toEqual([]);
    expect(harness.dispatched).toEqual([]);
    expect(harness.restoredEditorText()).toBeUndefined();
  });

  it("does not overwrite an existing editor draft", async () => {
    const user = messageEntry("user-1", null, "user", "Session prompt");
    const assistant = messageEntry("assistant-1", user.id, "assistant", "Old reply");
    const harness = await renderHarness({
      entries: [user, assistant],
      leafId: assistant.id,
      editorText: "Unsaved draft",
      navigate: async () => ({ editorText: "Session prompt" }),
    });

    await confirm(harness);
    await harness.setup.waitFor(() => harness.dispatched.length === 1);

    expect(harness.navigatedEntryIds).toEqual([user.id]);
    expect(harness.restoredEditorText()).toBeUndefined();
  });

  it("does not dispatch or restore editor text when Host navigation fails", async () => {
    const user = messageEntry("user-1", null, "user", "Session prompt");
    const assistant = messageEntry("assistant-1", user.id, "assistant", "Old reply");
    const harness = await renderHarness({
      entries: [user, assistant],
      leafId: assistant.id,
      navigate: async () => {
        throw new Error("navigation failed");
      },
    });

    await confirm(harness);
    await harness.setup.waitFor(() => harness.notifications.length === 1);

    expect(harness.closed()).toBe(true);
    expect(harness.dispatched).toEqual([]);
    expect(harness.restoredEditorText()).toBeUndefined();
    expect(harness.notifications[0]).toMatchObject({
      message: "Navigation failed: navigation failed",
      severity: "error",
    });
  });

  it("refreshes the canonical branch without restoring text for a Host no-op", async () => {
    const user = messageEntry("user-1", null, "user", "Current prompt");
    const harness = await renderHarness({
      entries: [user],
      leafId: user.id,
      branchEntries: [user],
      navigate: async () => ({}),
    });

    await confirm(harness);
    await harness.setup.waitFor(() => harness.dispatched.length === 1);

    expect(harness.navigatedEntryIds).toEqual([user.id]);
    expect(harness.restoredEditorText()).toBeUndefined();
    expect(harness.dispatched[0]).toMatchObject({
      type: "session_resumed",
      transcript: [{ id: user.id, role: "user", text: "Current prompt" }],
    });
  });
});
