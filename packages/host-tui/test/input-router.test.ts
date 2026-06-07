// ============================================================================
// InputRouter tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import type { KeyEvent } from "../src/focus/index.js";
import { FocusManager, InputRouter } from "../src/focus/index.js";

const key = (name: string): KeyEvent => ({
  name,
  ctrl: false,
  shift: false,
});

describe("InputRouter", () => {
  it("routes editor child interactions before focus owners", () => {
    const focus = new FocusManager();
    const calls: string[] = [];
    const router = new InputRouter({
      focus,
      getState: () => ({ surfaces: [] }),
      appFallback: () => {
        calls.push("app");
        return true;
      },
    });

    router.setEditorChildHandler((event) => {
      calls.push(`child:${event.name}`);
      return true;
    });

    expect(router.dispatch(key("down"))).toBe(true);
    expect(calls).toEqual(["child:down"]);
  });

  it("does not fall through to app fallback while a blocking surface is active", () => {
    const focus = new FocusManager();
    let appCalls = 0;
    const router = new InputRouter({
      focus,
      getState: () => ({ surfaces: [{ blocking: true }] }),
      appFallback: () => {
        appCalls += 1;
        return true;
      },
    });

    expect(router.dispatch(key("x"))).toBe(false);
    expect(appCalls).toBe(0);
  });

  it("uses app fallback when focus does not handle and no blocking surface exists", () => {
    const focus = new FocusManager();
    let appCalls = 0;
    const router = new InputRouter({
      focus,
      getState: () => ({ surfaces: [] }),
      appFallback: () => {
        appCalls += 1;
        return true;
      },
    });

    expect(router.dispatch(key("x"))).toBe(true);
    expect(appCalls).toBe(1);
  });
});
