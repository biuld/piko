// ============================================================================
// Surface occlusion — compute fully covered slots from active surfaces
// ============================================================================

import type { SurfaceSlot, TuiSurfaceState } from "./types.js";

/**
 * Compute which base slots are fully covered by active surfaces.
 * Returns the set of slots that should not be rendered.
 */
export function computeFullyCoveredSlots(surfaces: TuiSurfaceState[]): Set<SurfaceSlot> {
  const fullyCovered = new Set<SurfaceSlot>();

  for (const surface of surfaces) {
    if (surface.mount === "replace-slot") {
      for (const slot of surface.occlusion.fullyCovers) {
        fullyCovered.add(slot);
      }
    }
  }

  return fullyCovered;
}

/**
 * Compute the effective rendering order for surfaces and slots.
 * Returns sorted layers for rendering.
 */
export function computeSurfaceLayers(
  surfaces: TuiSurfaceState[],
): Array<{ surfaceId: string; zIndex: number }> {
  return surfaces
    .map((s) => ({ surfaceId: s.id, zIndex: s.zIndex }))
    .sort((a, b) => a.zIndex - b.zIndex);
}

/**
 * Check if a surface is visible (not fully covered by a higher surface).
 */
export function isSurfaceVisible(
  surface: TuiSurfaceState,
  allSurfaces: TuiSurfaceState[],
): boolean {
  const higherSurfaces = allSurfaces.filter((s) => s.zIndex > surface.zIndex);
  for (const higher of higherSurfaces) {
    if (
      higher.mount === "replace-slot" &&
      higher.occlusion.fullyCovers.includes("app" as SurfaceSlot)
    ) {
      return false;
    }
  }
  return true;
}
