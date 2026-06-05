import type { PanelSession } from "../panels/types.js";

// ============================================================================
// Surface types — Panel surface state
// ============================================================================

export type SurfaceSlot = "app" | "timeline" | "editor" | "status" | "bottom-bar";

export type SurfacePlacement = "partial" | "full";

export type SurfaceInputPolicy = "capture" | "passive";

export type SurfaceDismissPolicy = "route-pop-or-close" | "manual";

export interface SurfaceState {
  id: string;
  placement: SurfacePlacement;
  inputPolicy: SurfaceInputPolicy;
  dismissPolicy: SurfaceDismissPolicy;
  zIndex: number;
  panel: PanelSession;
  parentId?: string;
}

export interface PanelSurfaceRequest {
  placement: SurfacePlacement;
  inputPolicy?: SurfaceInputPolicy;
  dismissPolicy?: SurfaceDismissPolicy;
  panel: PanelSession;
}

export interface SurfaceContext {
  viewportWidth: number;
  viewportHeight: number;
  activeSurfaces: SurfaceState[];
  hasActiveStream: boolean;
}
