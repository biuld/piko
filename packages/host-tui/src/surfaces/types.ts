// ============================================================================
// Surface types — mount strategies, roles, occlusion, surface state
// ============================================================================

export type SurfaceMount =
  | "replace-slot"
  | "insert-between"
  | "anchored"
  | "side-drawer"
  | "status-line";

export type SurfaceSlot = "app" | "timeline" | "editor" | "status" | "bottom-bar";

export type SurfaceRole = "autocomplete" | "selector" | "menu" | "form" | "confirm" | "status";

export interface SurfaceOcclusion {
  covers: SurfaceSlot[];
  fullyCovers: SurfaceSlot[];
}

export type SurfaceInteractionOwner = "self" | "anchor" | "none";

export interface TuiSurfaceState {
  id: string;
  mount: SurfaceMount;
  role: SurfaceRole;
  zIndex: number;
  parentId?: string;
  anchorId?: string;
  targetSlot?: SurfaceSlot;
  insertAfterSlot?: SurfaceSlot;
  occlusion: SurfaceOcclusion;
  interactionOwner: SurfaceInteractionOwner;
  focusOwnerId?: string;
  blocking: boolean;
  data?: unknown;
}

export interface SurfaceRequest {
  role: SurfaceRole;
  preferredMount?: SurfaceMount;
  targetSlot?: SurfaceSlot;
  contentSize?: "small" | "medium" | "large";
  requiresSecretInput?: boolean;
  destructive?: boolean;
  parentId?: string;
  anchorId?: string;
  data?: unknown;
}

export interface SurfaceContext {
  viewportWidth: number;
  viewportHeight: number;
  activeSurfaces: TuiSurfaceState[];
  hasActiveStream: boolean;
}

export interface SurfaceLayer {
  surfaceId: string;
  zIndex: number;
  occlusion: SurfaceOcclusion;
}

export function createDefaultOcclusion(): SurfaceOcclusion {
  return { covers: [], fullyCovers: [] };
}
