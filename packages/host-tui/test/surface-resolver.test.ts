// ============================================================================
// Surface resolver unit tests — mount & slot resolution from role + viewport
// ============================================================================

import { describe, expect, it } from "vitest";
import { resolveSurface } from "../src/surfaces/surface-resolver.js";
import type { SurfaceContext, SurfaceRequest } from "../src/surfaces/types.js";

function makeCtx(viewportWidth: number): SurfaceContext {
  return {
    viewportWidth,
    viewportHeight: 40,
    activeSurfaces: [],
    hasActiveStream: false,
  };
}

function makeRequest(overrides: Partial<SurfaceRequest> = {}): SurfaceRequest {
  return {
    role: "selector",
    contentSize: "medium",
    ...overrides,
  };
}

// ============================================================================
// Mount resolution
// ============================================================================
describe("resolveSurface mount", () => {
  // -- selector / menu: small/medium → insert-between --
  it("medium selector → insert-between on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "medium" }),
      makeCtx(120),
    );
    expect(surface.mount).toBe("insert-between");
  });

  it("medium selector → insert-between on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "medium" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("insert-between");
  });

  it("small selector → insert-between on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "small" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("insert-between");
  });

  it("medium menu → insert-between", () => {
    const surface = resolveSurface(
      makeRequest({ role: "menu", contentSize: "medium" }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("insert-between");
  });

  // -- selector / menu: large → replace-slot timeline --
  it("large selector → replace-slot on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(130),
    );
    expect(surface.mount).toBe("replace-slot");
  });

  it("large selector → replace-slot on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("replace-slot");
  });

  it("large menu → replace-slot on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "menu", contentSize: "large" }),
      makeCtx(140),
    );
    expect(surface.mount).toBe("replace-slot");
  });

  it("large menu → replace-slot on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "menu", contentSize: "large" }),
      makeCtx(50),
    );
    expect(surface.mount).toBe("replace-slot");
  });

  // -- form: always insert-between --
  it("form → insert-between on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "form", contentSize: "small" }),
      makeCtx(120),
    );
    expect(surface.mount).toBe("insert-between");
  });

  it("form → insert-between on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "form", contentSize: "small" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("insert-between");
  });

  it("form + modal → replace-slot on narrow viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "form", modality: "modal", contentSize: "small" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("replace-slot");
    expect(surface.targetSlot).toBe("timeline");
  });

  it("form + modal → insert-between on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "form", modality: "modal", contentSize: "small" }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("insert-between");
  });

  // -- confirm: destructive → replace-slot, normal → insert-between --
  it("destructive confirm → replace-slot", () => {
    const surface = resolveSurface(
      makeRequest({ role: "confirm", destructive: true }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("replace-slot");
  });

  it("normal confirm → insert-between", () => {
    const surface = resolveSurface(makeRequest({ role: "confirm" }), makeCtx(100));
    expect(surface.mount).toBe("insert-between");
  });

  // -- status: always status-line --
  it("status → status-line", () => {
    const surface = resolveSurface(makeRequest({ role: "status" }), makeCtx(100));
    expect(surface.mount).toBe("status-line");
  });
});

// ============================================================================
// insertAfterSlot resolution
// ============================================================================
describe("resolveSurface insertAfterSlot", () => {
  it("medium selector → insertAfterSlot is 'status'", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "medium" }),
      makeCtx(100),
    );
    expect(surface.insertAfterSlot).toBe("status");
  });

  it("medium menu → insertAfterSlot is 'status'", () => {
    const surface = resolveSurface(
      makeRequest({ role: "menu", contentSize: "medium" }),
      makeCtx(100),
    );
    expect(surface.insertAfterSlot).toBe("status");
  });

  it("form → insertAfterSlot is 'status'", () => {
    const surface = resolveSurface(makeRequest({ role: "form" }), makeCtx(100));
    expect(surface.insertAfterSlot).toBe("status");
  });

  it("normal confirm → insertAfterSlot is 'status'", () => {
    const surface = resolveSurface(makeRequest({ role: "confirm" }), makeCtx(100));
    expect(surface.insertAfterSlot).toBe("status");
  });

  it("large replace-slot → insertAfterSlot is undefined on wide viewport", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(140),
    );
    expect(surface.insertAfterSlot).toBeUndefined();
  });

  it("replace-slot → insertAfterSlot is undefined", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(60),
    );
    expect(surface.insertAfterSlot).toBeUndefined();
  });

  it("status-line → insertAfterSlot is undefined", () => {
    const surface = resolveSurface(makeRequest({ role: "status" }), makeCtx(100));
    expect(surface.insertAfterSlot).toBeUndefined();
  });
});

// ============================================================================
// targetSlot resolution
// ============================================================================
describe("resolveSurface targetSlot", () => {
  it("insert-between command surface has no targetSlot", () => {
    const surface = resolveSurface(
      makeRequest({ role: "menu", contentSize: "medium" }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("insert-between");
    expect(surface.targetSlot).toBeUndefined();
  });

  it("non-destructive replace-slot targets timeline", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("replace-slot");
    expect(surface.targetSlot).toBe("timeline");
  });

  it("destructive replace-slot targets app", () => {
    const surface = resolveSurface(
      makeRequest({ role: "confirm", destructive: true }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("replace-slot");
    expect(surface.targetSlot).toBe("app");
  });
});

// ============================================================================
// Occlusion
// ============================================================================
describe("resolveSurface occlusion", () => {
  it("insert-between covers no slots fully", () => {
    const surface = resolveSurface(makeRequest({ role: "form" }), makeCtx(100));
    expect(surface.occlusion.fullyCovers).toEqual([]);
  });

  it("destructive confirm → replace-slot covers app fully", () => {
    const surface = resolveSurface(
      makeRequest({ role: "confirm", destructive: true }),
      makeCtx(100),
    );
    expect(surface.mount).toBe("replace-slot");
    expect(surface.occlusion.fullyCovers).toContain("timeline");
    expect(surface.occlusion.fullyCovers).toContain("editor");
  });

  it("large selector on narrow → replace-slot covers timeline", () => {
    const surface = resolveSurface(
      makeRequest({ role: "selector", contentSize: "large" }),
      makeCtx(60),
    );
    expect(surface.mount).toBe("replace-slot");
    expect(surface.occlusion.fullyCovers).toContain("timeline");
  });
});
