// ============================================================================
// Surfaces — public API
// ============================================================================

export type { RenderPlanEntry } from "./render-plan.js";
export { computeRenderPlan } from "./render-plan.js";
export { type SurfaceEvent, SurfaceManager } from "./surface-manager.js";
export {
  computeFullyCoveredSlots,
  computeSurfaceLayers,
  isSurfaceVisible,
} from "./surface-occlusion.js";
export { resolveSurface } from "./surface-resolver.js";
export type {
  SurfaceContext,
  SurfaceLayer,
  SurfaceMount,
  SurfaceOcclusion,
  SurfaceRequest,
  SurfaceRole,
  SurfaceSlot,
  TuiSurfaceState,
} from "./types.js";
export { createDefaultOcclusion } from "./types.js";
