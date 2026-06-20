import { afterEach, describe, expect, it } from "bun:test";
import { testRender } from "@opentui/solid";
import { flattenSessionTree } from "piko-host-runtime";
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
  onSelect?: (entryId: string) => Promise<void>;
  onCancel?: () => void;
}

async function renderHarness(options: HarnessOptions = {}) {
  const rawEntries = options.entries ?? [];
  const leafId = options.leafId ?? null;
  const { flat } = flattenSessionTree(rawEntries, leafId);

  let surfaceController:
    | {
        handleKey(event: { name: string; shift?: boolean }): { type: string };
        onConfirm?(): void;
      }
    | undefined;
  let cancelled = false;
  const selectedEntryIds: string[] = [];

  const controller = {
    setSurfaceController(_id: string, value: typeof surfaceController | null) {
      surfaceController = value ?? undefined;
    },
  };

  const setup = await testRender(
    () =>
      createComponent(TreeSelector, {
        entries: flat,
        leafId,
        loading: false,
        onSelect: async (entryId) => {
          selectedEntryIds.push(entryId);
          await options.onSelect?.(entryId);
        },
        onCancel: () => {
          cancelled = true;
          options.onCancel?.();
        },
        controller: controller as never,
        surfaceId: "tree-surface",
        availableWidth: 80,
        availableHeight: 20,
      }),
    { width: 80, height: 24 },
  );
  renderers.push(setup.renderer);
  await setup.waitFor(() => surfaceController !== undefined);
  await setup.flush();

  return {
    setup,
    controller: () => surfaceController!,
    cancelled: () => cancelled,
    selectedEntryIds,
  };
}

async function confirm(harness: Awaited<ReturnType<typeof renderHarness>>): Promise<void> {
  expect(harness.controller().handleKey({ name: "enter" }).type).toBe("confirm");
  harness.controller().onConfirm?.();
}

describe("TreeSelector navigation (pure)", () => {
  it("emits selected user entry ID on confirm", async () => {
    const user = messageEntry("user-1", null, "user", "Edit this prompt");
    const assistant = messageEntry("assistant-1", user.id, "assistant", "Old reply");

    let selectPromiseResolved = false;
    const harness = await renderHarness({
      entries: [user, assistant],
      leafId: assistant.id,
      onSelect: async (entryId) => {
        expect(entryId).toBe("user-1");
        selectPromiseResolved = true;
      },
    });

    await confirm(harness);
    await harness.setup.flush();

    expect(harness.selectedEntryIds).toEqual(["user-1"]);
    expect(selectPromiseResolved).toBe(true);
  });

  it("does nothing when the tree has no selectable user entry", async () => {
    const assistant = messageEntry("assistant-1", null, "assistant", "Only reply");
    const harness = await renderHarness({
      entries: [assistant],
      leafId: assistant.id,
    });

    await confirm(harness);
    await harness.setup.flush();

    expect(harness.selectedEntryIds).toEqual([]);
  });

  it("submitting guard: repeated Enter while submitting only calls onSelect once", async () => {
    const user = messageEntry("user-1", null, "user", "Edit this prompt");
    let callCount = 0;

    // A promise that resolves slowly
    let resolveSelect!: () => void;
    const selectPromise = new Promise<void>((resolve) => {
      resolveSelect = resolve;
    });

    const harness = await renderHarness({
      entries: [user],
      leafId: user.id,
      onSelect: async () => {
        callCount++;
        await selectPromise;
      },
    });

    // First confirm
    await confirm(harness);

    // Second confirm immediately (while selectPromise is pending)
    const secondResult = harness.controller().handleKey({ name: "enter" });
    expect(secondResult.type).toBe("handled"); // Guarded, type is handled instead of confirm!
    harness.controller().onConfirm?.();

    resolveSelect();
    await harness.setup.flush();

    expect(callCount).toBe(1);
  });

  it("Escape key triggers onCancel callback", async () => {
    const user = messageEntry("user-1", null, "user", "Edit this prompt");
    const harness = await renderHarness({
      entries: [user],
      leafId: user.id,
    });

    const result = harness.controller().handleKey({ name: "escape" });
    expect(result.type).toBe("handled");
    expect(harness.cancelled()).toBe(true);
  });
});
