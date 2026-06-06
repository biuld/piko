// ============================================================================
// Render plan — computes ordered render entries from state:
// base slots (timeline, status, editor, bottom-bar) + active panels.
// ============================================================================

import type { TuiState } from "../state/state.js";
import type { SurfaceState } from "./types.js";

export interface RenderPlanEntry {
  kind: "slot" | "surface";
  id: string;
  placement?: "partial" | "full";
  surface?: SurfaceState;
}

const SLOT_ENTRIES: Record<string, RenderPlanEntry> = {
  timeline: { kind: "slot", id: "timeline" },
  editor: { kind: "slot", id: "editor" },
  status: { kind: "slot", id: "status" },
  "bottom-bar": { kind: "slot", id: "bottom-bar" },
};

const SURFACE_ENTRIES = new WeakMap<SurfaceState, RenderPlanEntry>();

function getSurfaceEntry(surface: SurfaceState): RenderPlanEntry {
  let entry = SURFACE_ENTRIES.get(surface);
  if (!entry) {
    entry = { kind: "surface", id: surface.id, placement: surface.placement, surface };
    SURFACE_ENTRIES.set(surface, entry);
  }
  return entry;
}

export function computeRenderPlan(state: TuiState): {
  inline: RenderPlanEntry[];
} {
  const surfaces = state.surfaces;
  const inline: RenderPlanEntry[] = [];

  const topSurface = (items: SurfaceState[]): SurfaceState | undefined =>
    [...items].sort((a, b) => b.zIndex - a.zIndex)[0];

  const fullPanel = topSurface(surfaces.filter((s) => s.placement === "full"));
  const partialPanel = topSurface(surfaces.filter((s) => s.placement === "partial"));

  // 1. Timeline / Panel Region
  if (fullPanel) {
    // A full panel replaces the timeline entirely in the layout flow.
    // Since it takes up remaining space, it essentially acts as the main view.
    inline.push(getSurfaceEntry(fullPanel));
  } else {
    inline.push(SLOT_ENTRIES.timeline);
  }

  // 2. Status
  inline.push(SLOT_ENTRIES.status);

  // 3. Insert-between surfaces live after status and before editor.
  if (partialPanel) {
    inline.push(getSurfaceEntry(partialPanel));
  }

  // 4. Editor
  inline.push(SLOT_ENTRIES.editor);

  // 5. Bottom bar
  inline.push(SLOT_ENTRIES["bottom-bar"]);

  return { inline };
}
