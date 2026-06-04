// ============================================================================
// Render plan — computes ordered render entries from state:
// base slots (timeline, status, editor, bottom-bar) + active surfaces,
// with occlusion culling and insert-between placement.
// ============================================================================

import type { TuiState } from "../state/state.js";
import type { SurfaceMount, SurfaceSlot, TuiSurfaceState } from "./types.js";

export interface RenderPlanEntry {
  kind: "slot" | "surface";
  /** Slot name for base slots, surface id for surfaces */
  id: string;
  mount?: SurfaceMount;
  /** The surface data (only for kind === "surface") */
  surface?: TuiSurfaceState;
}

/**
 * Compute the ordered render plan from current state.
 * Pure function — testable without any renderer.
 *
 * Layout order (top to bottom):
 *   1. timeline slot (if not fully covered)
 *   2. insert-between surfaces after timeline
 *   3. status slot (if not fully covered)
 *   4. anchored surfaces
 *   5. insert-between surfaces after status
 *   6. editor slot (if not fully covered)
 *   7. insert-between surfaces after editor
 *   8. replace-slot surfaces
 *   9. bottom-bar slot (if not fully covered)
 *   10. side-drawer surfaces (render via Portal)
 */
export function computeRenderPlan(state: TuiState): {
  /** Ordered entries for inline rendering */
  inline: RenderPlanEntry[];
  /** Side-drawer surfaces (rendered via Portal) */
  drawers: RenderPlanEntry[];
} {
  const surfaces = state.surfaces;

  // Compute fully covered slots from replace-slot + side-drawer on narrow
  const fullyCovered = computeFullyCovered(surfaces, state.layout.viewport.width);

  const inline: RenderPlanEntry[] = [];
  const drawers: RenderPlanEntry[] = [];

  // Helper: check if a slot is visible
  const slotVisible = (slot: SurfaceSlot) =>
    !fullyCovered.has(slot) && !fullyCovered.has("app" as SurfaceSlot);

  // Helper: filter surfaces by mount + optional insertAfterSlot
  const filterSurfaces = (mount: SurfaceMount, after?: SurfaceSlot): TuiSurfaceState[] =>
    surfaces.filter((s: TuiSurfaceState) => {
      if (s.mount !== mount) return false;
      if (after !== undefined && s.insertAfterSlot !== after) return false;
      return true;
    });

  // 1. Timeline
  if (slotVisible("timeline")) {
    inline.push({ kind: "slot", id: "timeline" });
  }
  for (const s of filterSurfaces("insert-between", "timeline")) {
    inline.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }

  // 2. Status line
  if (slotVisible("status")) {
    inline.push({ kind: "slot", id: "status" });
  }
  for (const s of filterSurfaces("insert-between", "status")) {
    inline.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }

  // 3. Editor
  if (slotVisible("editor")) {
    inline.push({ kind: "slot", id: "editor" });
  }
  for (const s of filterSurfaces("insert-between", "editor")) {
    inline.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }

  // 4. Replace-slot surfaces
  for (const s of filterSurfaces("replace-slot")) {
    inline.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }

  // 5. Bottom bar
  if (slotVisible("bottom-bar")) {
    inline.push({ kind: "slot", id: "bottom-bar" });
  }

  // 6. Anchored + side-drawer surfaces render as overlays (no layout shift)
  for (const s of filterSurfaces("anchored")) {
    drawers.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }
  for (const s of filterSurfaces("side-drawer")) {
    drawers.push({ kind: "surface", id: s.id, mount: s.mount, surface: s });
  }

  return { inline, drawers };
}

/**
 * Compute the set of fully covered base slots.
 */
function computeFullyCovered(surfaces: TuiSurfaceState[], viewportWidth: number): Set<SurfaceSlot> {
  const fullyCovered = new Set<SurfaceSlot>();

  for (const s of surfaces) {
    if (s.mount === "replace-slot") {
      const target = s.targetSlot ?? "app";
      if (target === "app") {
        fullyCovered.add("timeline");
        fullyCovered.add("editor");
        fullyCovered.add("status");
        fullyCovered.add("bottom-bar");
      } else {
        fullyCovered.add(target);
      }
    }
    // side-drawer on narrow terminals fully covers timeline
    if (s.mount === "side-drawer" && viewportWidth < 80) {
      fullyCovered.add("timeline");
    }
  }

  return fullyCovered;
}
