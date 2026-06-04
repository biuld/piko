// ============================================================================
// EditorAutocompleteController unit tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type {
  AutocompleteItem,
  AutocompleteProvider,
  AutocompleteSuggestions,
} from "../src/autocomplete/types.js";
import { EditorAutocompleteController } from "../src/editor/editor-autocomplete-controller.js";
import { createEmptyAutocompleteState } from "../src/editor/editor-autocomplete-state.js";

// ---- Fake provider that returns static items ----
class FakeAutocompleteProvider implements AutocompleteProvider {
  constructor(
    private items: AutocompleteItem[],
    private delay = 0,
    private prefix = "",
    private providerId = "fake",
  ) {}

  async getSuggestions(
    _input: string,
    _cursor: number,
    _options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null> {
    if (this.delay > 0) {
      await new Promise((r) => setTimeout(r, this.delay));
    }
    if (_options.signal.aborted) return null;
    return {
      items: this.items,
      prefix: this.prefix,
      providerId: this.providerId,
    };
  }

  applyCompletion(
    input: string,
    _cursor: number,
    item: AutocompleteItem,
    _prefix: string,
  ): { input: string; cursor: number } {
    const prefix = _prefix || this.prefix;
    const stripped = input.slice(
      0,
      input.lastIndexOf(prefix) >= 0 ? input.lastIndexOf(prefix) : input.length,
    );
    const newInput = stripped + item.value;
    return { input: newInput, cursor: newInput.length };
  }
}

// ---- Fake CombinedAutocompleteProvider adapter ----
// The EditorAutocompleteController constructor expects CombinedAutocompleteProvider,
// but we can cast since the test only uses the provider interface methods.
function makeFakeCombined(provider: FakeAutocompleteProvider): any {
  return {
    getSuggestions: provider.getSuggestions.bind(provider),
    applyCompletion: provider.applyCompletion.bind(provider),
  };
}

function makeController(items: AutocompleteItem[], delay = 0, prefix = "/", providerId = "slash") {
  const provider = new FakeAutocompleteProvider(items, delay, prefix, providerId);
  let lastState = createEmptyAutocompleteState();
  const controller = new EditorAutocompleteController(
    makeFakeCombined(provider) as any,
    (s) => {
      lastState = s;
    },
    undefined,
    (input: string): AutocompleteItem[] => {
      // Sync fallback: return static items for "/" prefix
      if (input.trimStart().startsWith("/")) {
        return items.map((it) => ({ ...it, providerId: "slash" }));
      }
      return [];
    },
  );
  return { controller, getState: () => lastState };
}

describe("EditorAutocompleteController", () => {
  it("initializes with empty state", () => {
    const { controller } = makeController([]);
    expect(controller.state.visible).toBe(false);
    expect(controller.state.items).toEqual([]);
    expect(controller.state.selectedIndex).toBe(0);
  });

  it("queries provider and updates state with results", async () => {
    const { controller, getState } = makeController([
      { value: "/model", label: "/model", description: "Select model" },
      { value: "/thinking", label: "/thinking", description: "Thinking level" },
    ]);
    await controller.query("/mod", "/mod".length);
    const state = getState();
    expect(state.visible).toBe(true);
    expect(state.loading).toBe(false);
    expect(state.items.length).toBe(2);
    expect(state.items[0].value).toBe("/model");
  });

  it("move() clamps selection to visible items", () => {
    const { controller, getState } = makeController([
      { value: "/a", label: "/a" },
      { value: "/b", label: "/b" },
      { value: "/c", label: "/c" },
    ]);

    // query to populate items
    controller.query("/x", "/x".length);

    controller.move(2);
    expect(getState().selectedIndex).toBe(2);

    controller.move(5); // beyond end
    expect(getState().selectedIndex).toBe(2);

    controller.move(-3);
    expect(getState().selectedIndex).toBe(0);

    controller.move(-1); // before start
    expect(getState().selectedIndex).toBe(0);
  });

  it("accept() returns completion result", async () => {
    const { controller, getState } = makeController(
      [{ value: "/model", label: "/model" }],
      0,
      "/",
      "slash",
    );
    await controller.query("/mod", "/mod".length);

    // Accept the first item
    const result = controller.accept();
    const state = getState();

    expect(result).not.toBeNull();
    expect(result!.input).toBe("/model");
    expect(state.selectedIndex).toBe(0);
  });

  it("accept() returns null when no items", () => {
    const { controller } = makeController([]);
    controller.query("/", "/".length);
    expect(controller.accept()).toBeNull();
  });

  it("cancel() resets state", async () => {
    const { controller, getState } = makeController([{ value: "/model", label: "/model" }]);
    await controller.query("/mod", "/mod".length);
    expect(getState().visible).toBe(true);

    controller.cancel();
    expect(getState().visible).toBe(false);
    expect(getState().items).toEqual([]);
    expect(getState().selectedIndex).toBe(0);
  });

  it("getSelectedItem() returns current item", async () => {
    const { controller } = makeController([
      { value: "/a", label: "/a" },
      { value: "/b", label: "/b" },
    ]);
    await controller.query("/x", "/x".length);
    controller.move(1);
    const item = controller.getSelectedItem();
    expect(item).not.toBeNull();
    expect(item!.value).toBe("/b");
  });

  it("getSelectedItem() returns null when no visible items", () => {
    const { controller } = makeController([]);
    expect(controller.getSelectedItem()).toBeNull();
  });

  it("isSlashProvider() returns true for slash provider", async () => {
    const { controller } = makeController([{ value: "/model", label: "/model" }], 0, "/", "slash");
    await controller.query("/mod", "/mod".length);
    expect(controller.isSlashProvider()).toBe(true);
  });

  it("visibleItems includes sync fallback when loading and async items empty", () => {
    const { controller } = makeController([
      { value: "/model", label: "/model" },
      { value: "/thinking", label: "/thinking" },
    ]);
    // Trigger query (async items not yet loaded)
    controller.query("/mod", "/mod".length);

    // visibleItems should return fallback during loading
    const visible = controller.visibleItems;
    expect(visible.length).toBeGreaterThan(0);
    // All items should have providerId from fallback
    expect(visible[0].providerId).toBe("slash");
  });

  it("visibleItems prefers async results over fallback", async () => {
    const asyncItems = [{ value: "/async-model", label: "/async-model" }];
    const { controller } = makeController(asyncItems, 0, "/", "slash");

    controller.query("/a", "/a".length);
    // async resolves immediately (delay=0), so visibleItems should be async items
    // Wait for microtask
    await new Promise((r) => setTimeout(r, 5));

    const visible = controller.visibleItems;
    expect(visible).toHaveLength(1);
    expect(visible[0].value).toBe("/async-model");
  });

  it("dispose() aborts inflight request", async () => {
    const { controller } = makeController([{ value: "/x", label: "/x" }], 100);
    const promise = controller.query("/x", "/x".length);
    controller.dispose();
    await promise; // should resolve without error after dispose aborts
    // After dispose+cancel, state should be empty
    expect(controller.state.loading).toBe(false);
  });
});
