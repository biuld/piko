// ============================================================================
// Render plan unit tests — slot replacement ordering
// ============================================================================

import { describe, expect, it } from "bun:test";
import { computeRenderPlan } from "../src/surfaces/render-plan.js";
import type { SurfaceState } from "../src/surfaces/types.js";

function makeState(surfaces: SurfaceState[] = []): any {
  return {
    surfaces,
    approval: { queue: [] },
    layout: { viewport: { width: 100, height: 40 } },
  };
}

function makeSurface(overrides: Partial<SurfaceState>): SurfaceState {
  return {
    id: "surface-1",
    placement: "partial",
    inputPolicy: "passive",
    dismissPolicy: "route-pop-or-close",
    zIndex: 10,
    panel: {} as any,
    ...overrides,
  };
}

describe("computeRenderPlan layout flow", () => {
  it("renders timeline, status, editor, and bottom-bar when no surfaces are active", () => {
    const plan = computeRenderPlan(makeState([]));
    expect(plan.inline.map((entry) => entry.id)).toEqual([
      "timeline",
      "status",
      "editor",
      "bottom-bar",
    ]);
  });

  it("renders full panel replacing timeline when a full panel is active", () => {
    const surface = makeSurface({ id: "full-surface", placement: "full" });
    const plan = computeRenderPlan(makeState([surface]));
    expect(plan.inline.map((entry) => entry.id)).toEqual([
      "full-surface",
      "status",
      "editor",
      "bottom-bar",
    ]);
  });

  it("renders partial panel after status when a partial panel is active", () => {
    const surface = makeSurface({ id: "partial-surface", placement: "partial" });
    const plan = computeRenderPlan(makeState([surface]));
    expect(plan.inline.map((entry) => entry.id)).toEqual([
      "timeline",
      "status",
      "partial-surface",
      "editor",
      "bottom-bar",
    ]);
  });

  it("renders topmost full panel when multiple are active", () => {
    const s1 = makeSurface({ id: "full-surface-1", placement: "full", zIndex: 10 });
    const s2 = makeSurface({ id: "full-surface-2", placement: "full", zIndex: 20 });
    const plan = computeRenderPlan(makeState([s1, s2]));
    expect(plan.inline.map((entry) => entry.id)).toEqual([
      "full-surface-2",
      "status",
      "editor",
      "bottom-bar",
    ]);
  });

  it("replaces status and editor with the approval panel", () => {
    const state = makeState([]);
    state.approval.pending = { callId: "call-1", toolName: "bash", toolArgs: {} };
    const plan = computeRenderPlan(state);
    expect(plan.inline.map((entry) => entry.id)).toEqual(["timeline", "approval", "bottom-bar"]);
  });

  it("keeps approval visible ahead of an existing partial surface", () => {
    const state = makeState([makeSurface({ id: "model-panel", inputPolicy: "capture" })]);
    state.approval.pending = { callId: "call-1", toolName: "edit", toolArgs: {} };
    const plan = computeRenderPlan(state);
    expect(plan.inline.map((entry) => entry.id)).toEqual(["timeline", "approval", "bottom-bar"]);
  });
});
