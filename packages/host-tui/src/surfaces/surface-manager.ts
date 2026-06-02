// ============================================================================
// SurfaceManager — surface lifecycle, mount resolution, occlusion, z-order
// ============================================================================

import {
  computeFullyCoveredSlots,
  computeSurfaceLayers,
  isSurfaceVisible,
} from "./surface-occlusion.js";
import { resolveSurface } from "./surface-resolver.js";
import type { SurfaceContext, SurfaceRequest, SurfaceSlot, TuiSurfaceState } from "./types.js";

export class SurfaceManager {
  private surfaces: TuiSurfaceState[] = [];
  private listeners: Array<(event: SurfaceEvent) => void> = [];

  /**
   * Subscribe to surface events.
   */
  onEvent(fn: (event: SurfaceEvent) => void): () => void {
    this.listeners.push(fn);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== fn);
    };
  }

  private emit(event: SurfaceEvent): void {
    for (const l of this.listeners) l(event);
  }

  /**
   * Open a surface from a request. Returns the surface ID.
   */
  open(request: SurfaceRequest, context?: SurfaceContext): string {
    const parentZIndex = request.parentId
      ? (this.surfaces.find((s) => s.id === request.parentId)?.zIndex ?? 0)
      : this.maxZIndex();

    const ctx: SurfaceContext = context ?? {
      viewportWidth: 80,
      viewportHeight: 24,
      activeSurfaces: this.surfaces,
      hasActiveStream: false,
    };

    const surface = resolveSurface(request, ctx, parentZIndex);
    this.surfaces.push(surface);

    this.emit({ type: "surface_opened", surface });
    return surface.id;
  }

  /**
   * Close a surface by ID, or the topmost surface if no ID given.
   * Also closes all child surfaces.
   */
  close(id?: string): void {
    if (id) {
      // Close the surface and its descendants
      const toClose = this.collectDescendants(id);
      toClose.add(id);
      const closed = this.surfaces.filter((s) => toClose.has(s.id));
      this.surfaces = this.surfaces.filter((s) => !toClose.has(s.id));
      for (const s of closed) {
        this.emit({ type: "surface_closed", surfaceId: s.id });
      }
    } else {
      // Close all surfaces
      const closed = [...this.surfaces];
      this.surfaces = [];
      for (const s of closed) {
        this.emit({ type: "surface_closed", surfaceId: s.id });
      }
    }
  }

  /**
   * Collect all descendant IDs of a surface.
   */
  private collectDescendants(parentId: string): Set<string> {
    const result = new Set<string>();
    for (const s of this.surfaces) {
      if (s.parentId === parentId) {
        result.add(s.id);
        const deeper = this.collectDescendants(s.id);
        for (const d of deeper) result.add(d);
      }
    }
    return result;
  }

  /**
   * Get all visible surfaces (not fully occluded).
   */
  getVisibleSurfaces(): TuiSurfaceState[] {
    return this.surfaces.filter((s) => isSurfaceVisible(s, this.surfaces));
  }

  /**
   * Get all active surfaces.
   */
  getAllSurfaces(): TuiSurfaceState[] {
    return [...this.surfaces];
  }

  /**
   * Get a specific surface by ID.
   */
  getSurface(id: string): TuiSurfaceState | undefined {
    return this.surfaces.find((s) => s.id === id);
  }

  /**
   * Get the currently blocking surface (highest z-index blocking surface).
   */
  getBlockingSurface(): TuiSurfaceState | undefined {
    return [...this.surfaces].filter((s) => s.blocking).sort((a, b) => b.zIndex - a.zIndex)[0];
  }

  /**
   * Compute which base slots are fully covered and should not render.
   */
  getFullyCoveredSlots(): Set<SurfaceSlot> {
    return computeFullyCoveredSlots(this.surfaces);
  }

  /**
   * Get sorted surface layers for rendering.
   */
  getSurfaceLayers() {
    return computeSurfaceLayers(this.surfaces);
  }

  /**
   * Get context for surface resolution.
   */
  getContext(
    viewportWidth: number,
    viewportHeight: number,
    hasActiveStream: boolean,
  ): SurfaceContext {
    return {
      viewportWidth,
      viewportHeight,
      activeSurfaces: this.surfaces,
      hasActiveStream,
    };
  }

  /**
   * Get the highest z-index among active surfaces.
   */
  private maxZIndex(): number {
    if (this.surfaces.length === 0) return 0;
    return Math.max(...this.surfaces.map((s) => s.zIndex));
  }
}

// ============================================================================
// Surface events
// ============================================================================

export type SurfaceEvent =
  | { type: "surface_opened"; surface: TuiSurfaceState }
  | { type: "surface_closed"; surfaceId: string };
