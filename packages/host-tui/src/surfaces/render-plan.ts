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
 *   1. timeline slot or its replace-slot surface
 *   2. insert-between surfaces after timeline
 *   3. status slot or its replace-slot surface
 *   4. anchored surfaces
 *   5. insert-between surfaces after status
 *   6. editor slot or its replace-slot surface
 *   7. insert-between surfaces after editor
 *   8. bottom-bar slot or its replace-slot surface
 *   9. side-drawer surfaces (render via Portal)
 */
// Stable objects for slots to prevent SolidJS <For> remounting
const SLOT_ENTRIES: Record<string, RenderPlanEntry> = {
  timeline: { kind: "slot", id: "timeline" },
  editor: { kind: "slot", id: "editor" },
  status: { kind: "slot", id: "status" },
  "bottom-bar": { kind: "slot", id: "bottom-bar" },
};

// Cache for surface entries
const SURFACE_ENTRIES = new WeakMap<TuiSurfaceState, RenderPlanEntry>();

function getSurfaceEntry(surface: TuiSurfaceState): RenderPlanEntry {
  let entry = SURFACE_ENTRIES.get(surface);
  if (!entry) {
    entry = { kind: "surface", id: surface.id, mount: surface.mount, surface };
    SURFACE_ENTRIES.set(surface, entry);
  }
  return entry;
}

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

  const replacementFor = (slot: SurfaceSlot): TuiSurfaceState | undefined => {
    const replacements = surfaces.filter(
      (s: TuiSurfaceState) => s.mount === "replace-slot" && (s.targetSlot ?? "app") === slot,
    );
    return replacements.sort((a, b) => b.zIndex - a.zIndex)[0];
  };

  const appReplacement = replacementFor("app");
  if (appReplacement) {
    inline.push({
      kind: "surface",
      id: appReplacement.id,
      mount: appReplacement.mount,
      surface: appReplacement,
    });
    return { inline, drawers };
  }

  // 1. Timeline
  const timelineReplacement = replacementFor("timeline");
  if (timelineReplacement) {
    inline.push(getSurfaceEntry(timelineReplacement));
  } else if (slotVisible("timeline")) {
    inline.push(SLOT_ENTRIES.timeline);
  }
  for (const s of filterSurfaces("insert-between", "timeline")) {
    inline.push(getSurfaceEntry(s));
  }

  // 2. Status
  const statusReplacement = replacementFor("status");
  if (statusReplacement) {
    inline.push(getSurfaceEntry(statusReplacement));
  } else if (slotVisible("status")) {
    inline.push(SLOT_ENTRIES.status);
  }
  for (const s of filterSurfaces("insert-between", "status")) {
    inline.push(getSurfaceEntry(s));
  }

  // 3. Editor
  const editorReplacement = replacementFor("editor");
  if (editorReplacement) {
    inline.push(getSurfaceEntry(editorReplacement));
  } else if (slotVisible("editor")) {
    inline.push(SLOT_ENTRIES.editor);
  }
  for (const s of filterSurfaces("insert-between", "editor")) {
    inline.push(getSurfaceEntry(s));
  }

  // 4. Bottom bar
  const bottomBarReplacement = replacementFor("bottom-bar");
  if (bottomBarReplacement) {
    inline.push(getSurfaceEntry(bottomBarReplacement));
  } else if (slotVisible("bottom-bar")) {
    inline.push(SLOT_ENTRIES["bottom-bar"]);
  }

  // 5. Anchored + side-drawer surfaces render as overlays (no layout shift)
  for (const s of filterSurfaces("anchored")) {
    drawers.push(getSurfaceEntry(s));
  }
  for (const s of filterSurfaces("side-drawer")) {
    drawers.push(getSurfaceEntry(s));
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
