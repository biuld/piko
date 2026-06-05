// ============================================================================
// Focus & Keyboard Routing Integration Tests
// ============================================================================

import { describe, expect, it } from "vitest";
import { FocusManager } from "../src/focus/focus-manager.js";

describe("Focus / Interaction Stack Routing", () => {
  it("initial stack state contains only editor", () => {
    const focus = new FocusManager();
    const state = focus.getState();
    expect(state.stack).toEqual(["editor"]);
    expect(state.activeOwnerId).toBe("editor");
  });

  it("pushing blocking surface updates stack", () => {
    const focus = new FocusManager();
    focus.registerOwner({
      id: "surface-1",
      region: "surface",
      priority: 10,
    });
    focus.pushFocus("surface-1", "surface");

    const state = focus.getState();
    expect(state.stack).toEqual(["editor", "surface-1"]);
    expect(state.activeOwnerId).toBe("surface-1");
  });

  it("closing surface pops focus back to parent", () => {
    const focus = new FocusManager();
    focus.registerOwner({
      id: "surface-1",
      region: "surface",
      priority: 10,
    });
    focus.pushFocus("surface-1", "surface", "editor");
    expect(focus.getState().activeOwnerId).toBe("surface-1");

    focus.closeSurface("surface-1");
    expect(focus.getState().stack).toEqual(["editor"]);
    expect(focus.getState().activeOwnerId).toBe("editor");
  });

  it("bubbles unhandled keys from top to bottom of stack", () => {
    const focus = new FocusManager();
    let parentReceived = false;
    let childReceived = false;

    focus.registerOwner({
      id: "editor",
      region: "editor",
      priority: 0,
      handleKey: (event) => {
        if (event.name === "x") {
          parentReceived = true;
          return { handled: true };
        }
        return { handled: false };
      },
    });

    focus.registerOwner({
      id: "editor.autocomplete",
      region: "editor",
      priority: 20,
      handleKey: (event) => {
        if (event.name === "up") {
          childReceived = true;
          return { handled: true };
        }
        return { handled: false };
      },
    });

    focus.pushFocus("editor.autocomplete", "editor");

    // Event handled by child
    const resChild = focus.handleKey({ name: "up", ctrl: false, shift: false });
    expect(resChild).toBe(true);
    expect(childReceived).toBe(true);
    expect(parentReceived).toBe(false);

    // Event bubbled to parent
    const resParent = focus.handleKey({ name: "x", ctrl: false, shift: false });
    expect(resParent).toBe(true);
    expect(parentReceived).toBe(true);
  });

  it("escape exits nested edit mode before closing settings", () => {
    const focus = new FocusManager();
    let settingsClosed = false;
    let editModeExited = false;

    // 1. Settings surface handles keys
    focus.registerOwner({
      id: "settings",
      region: "surface",
      priority: 10,
      handleKey: (event) => {
        if (event.name === "escape") {
          settingsClosed = true;
          return { handled: true };
        }
        return { handled: false };
      },
    });
    focus.pushFocus("settings", "surface");

    // 2. Settings edit mode pushed on top
    focus.registerOwner({
      id: "settings.edit",
      region: "surface",
      priority: 20,
      handleKey: (event) => {
        if (event.name === "escape") {
          editModeExited = true;
          // Return push/pop result to pop off stack
          return { pop: true };
        }
        return { handled: false };
      },
    });
    focus.pushFocus("settings.edit", "surface");

    expect(focus.getState().stack).toEqual(["editor", "settings", "settings.edit"]);

    // First Esc exits edit mode
    const esc1 = focus.handleKey({ name: "escape", ctrl: false, shift: false });
    expect(esc1).toBe(true);
    expect(editModeExited).toBe(true);
    expect(settingsClosed).toBe(false);
    expect(focus.getState().stack).toEqual(["editor", "settings"]);

    // Second Esc closes settings
    const esc2 = focus.handleKey({ name: "escape", ctrl: false, shift: false });
    expect(esc2).toBe(true);
    expect(settingsClosed).toBe(true);
  });
});
