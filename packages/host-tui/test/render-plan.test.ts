// ============================================================================
// Render plan unit tests — slot replacement ordering
// ============================================================================

import { describe, expect, it } from "vitest";
import { computeRenderPlan } from "../src/surfaces/render-plan.js";
import type { TuiSurfaceState } from "../src/surfaces/types.js";

function makeState(surfaces: TuiSurfaceState[] = []): any {
  return {
    surfaces,
    layout: { viewport: { width: 100, height: 40 } },
  };
}

function makeSurface(overrides: Partial<TuiSurfaceState>): TuiSurfaceState {
  return {
    id: "surface-1",
    mount: "replace-slot",
    role: "selector",
    zIndex: 10,
    targetSlot: "timeline",
    occlusion: { covers: ["timeline"], fullyCovers: ["timeline"] },
    interactionOwner: "self",
    blocking: true,
    data: { type: "resume" },
    ...overrides,
  };
}

describe("computeRenderPlan replace-slot ordering", () => {
  it("renders timeline replacement before status and editor", () => {
    const surface = makeSurface({ id: "resume-surface", targetSlot: "timeline" });
    const plan = computeRenderPlan(makeState([surface]));

    expect(plan.inline.map((entry) => entry.id)).toEqual([
      "resume-surface",
      "status",
      "editor",
      "bottom-bar",
    ]);
  });

  it("renders destructive app replacement as the only inline entry", () => {
    const surface = makeSurface({
      id: "confirm-surface",
      role: "confirm",
      targetSlot: "app",
      occlusion: {
        covers: ["timeline", "editor", "status", "bottom-bar"],
        fullyCovers: ["timeline", "editor", "status", "bottom-bar"],
      },
    });
    const plan = computeRenderPlan(makeState([surface]));

    expect(plan.inline.map((entry) => entry.id)).toEqual(["confirm-surface"]);
  });
});
