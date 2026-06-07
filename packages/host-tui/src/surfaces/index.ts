// ============================================================================
// Surfaces — public API
// ============================================================================

export {
  confirmBehavior,
  formBehavior,
  menuBehavior,
  type SurfaceKeyResult,
  selectorBehavior,
} from "./interactions/role-behavior.js";
export type { RenderPlanEntry } from "./render-plan.js";
export { computeRenderPlan } from "./render-plan.js";
export { type SurfaceEvent, SurfaceManager } from "./surface-manager.js";
export type {
  PanelSurfaceRequest,
  SurfaceContext,
  SurfaceDismissPolicy,
  SurfaceInputPolicy,
  SurfacePlacement,
  SurfaceSlot,
  SurfaceState,
} from "./types.js";
