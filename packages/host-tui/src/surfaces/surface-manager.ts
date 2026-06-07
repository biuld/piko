// ============================================================================
// SurfaceManager — surface lifecycle, mount resolution, z-order
// ============================================================================

import type { PanelSurfaceRequest, SurfaceContext, SurfaceState } from "./types.js";

let surfaceIdCounter = 0;
function nextId(prefix = "surface"): string {
  return `${prefix}-${++surfaceIdCounter}`;
}

export class SurfaceManager {
  private surfaces: SurfaceState[] = [];
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
  openPanel(request: PanelSurfaceRequest): string {
    const zIndex = this.maxZIndex() + 10;
    const surface: SurfaceState = {
      id: nextId(),
      placement: request.placement,
      inputPolicy: request.inputPolicy ?? "capture",
      dismissPolicy: request.dismissPolicy ?? "route-pop-or-close",
      zIndex,
      panel: request.panel,
    };
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
   * Get all active surfaces.
   */
  getAllSurfaces(): SurfaceState[] {
    return [...this.surfaces];
  }

  /**
   * Get a specific surface by ID.
   */
  getSurface(id: string): SurfaceState | undefined {
    return this.surfaces.find((s) => s.id === id);
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
  | { type: "surface_opened"; surface: SurfaceState }
  | { type: "surface_closed"; surfaceId: string };
