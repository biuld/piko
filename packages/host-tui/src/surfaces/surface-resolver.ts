// ============================================================================
// Surface resolver — mount strategy resolution from role, viewport, content
// ============================================================================

import type {
  SurfaceContext,
  SurfaceInteractionOwner,
  SurfaceModality,
  SurfaceMount,
  SurfaceRequest,
  SurfaceRole,
  SurfaceSlot,
  TuiSurfaceState,
} from "./types.js";
import { createDefaultOcclusion } from "./types.js";

let surfaceIdCounter = 0;

function nextId(): string {
  return `surface-${++surfaceIdCounter}`;
}

/**
 * Resolve a surface request into a full TuiSurfaceState using context.
 */
export function resolveSurface(
  request: SurfaceRequest,
  context: SurfaceContext,
  parentZIndex = 0,
): TuiSurfaceState {
  const modality = request.modality ?? defaultModality(request.role);
  const mount = resolveMount(request, context);
  const targetSlot = resolveTargetSlot(request, mount);
  const occlusion = deriveOcclusion(mount, request, context);
  const zIndex = parentZIndex + 10; // Child surfaces are higher

  return {
    id: nextId(),
    mount,
    role: request.role,
    modality,
    zIndex,
    parentId: request.parentId,
    anchorId: request.anchorId,
    targetSlot,
    insertAfterSlot: resolveInsertAfterSlot(request, mount),
    occlusion,
    interactionOwner: resolveInteractionOwner(request.role),
    blocking: isBlocking(request.role, modality),
    data: request.data,
  };
}

function defaultModality(role: SurfaceRole): SurfaceModality {
  if (role === "status") return "nonblocking";
  return "blocking";
}

/**
 * Resolve the mount strategy based on role, content size, and viewport.
 *
 * Rules (no command preference — layout is the resolver's job):
 *
 *   selector / menu:
 *     small/medium → insert-between after status
 *     large        → replace-slot timeline
 *   form:
 *     modal        → replace-slot timeline (narrow) or insert-between (wide)
 *     blocking     → insert-between after status
 *   confirm:
 *     destructive  → replace-slot app
 *     normal       → insert-between after status
 *   status:
 *     always       → status-line
 */
function resolveMount(request: SurfaceRequest, context: SurfaceContext): SurfaceMount {
  const modality = request.modality ?? defaultModality(request.role);

  switch (request.role) {
    case "selector":
    case "menu": {
      if (request.contentSize === "large") {
        return "replace-slot";
      }
      // small / medium → always insert-between
      return "insert-between";
    }

    case "form": {
      if (modality === "modal") {
        return context.viewportWidth < 80 ? "replace-slot" : "insert-between";
      }
      return "insert-between";
    }

    case "confirm": {
      if (request.destructive) return "replace-slot";
      return "insert-between";
    }

    case "status":
      return "status-line";

    default:
      return "insert-between";
  }
}

/**
 * Determine which base slot a replacing surface owns.
 * Command requests do not provide layout placement; resolver owns it.
 */
function resolveTargetSlot(request: SurfaceRequest, mount: SurfaceMount): SurfaceSlot | undefined {
  if (mount !== "replace-slot") return undefined;
  const modality = request.modality ?? defaultModality(request.role);
  if (modality === "modal") {
    if (request.role === "form") {
      return "timeline";
    }
    if (request.role === "confirm" && request.destructive) {
      return "app";
    }
  }
  return request.destructive ? "app" : "timeline";
}

/**
 * Determine the insertAfterSlot for insert-between mounts.
 * Always returns "status" — panel sits between status bar and editor.
 */
function resolveInsertAfterSlot(
  _request: SurfaceRequest,
  mount: SurfaceMount,
): SurfaceSlot | undefined {
  if (mount !== "insert-between") return undefined;
  return "status";
}

/**
 * Derive occlusion from mount strategy.
 */
function deriveOcclusion(
  mount: SurfaceMount,
  request: SurfaceRequest,
  _context: SurfaceContext,
): { covers: SurfaceSlot[]; fullyCovers: SurfaceSlot[] } {
  switch (mount) {
    case "replace-slot": {
      // Destructive confirm covers app; other replace-slot covers timeline
      const target = request.destructive ? "app" : "timeline";
      if (target === "app") {
        return {
          covers: ["timeline", "editor", "status", "bottom-bar"],
          fullyCovers: ["timeline", "editor", "status"],
        };
      }
      return {
        covers: [target],
        fullyCovers: [target],
      };
    }

    case "insert-between":
      // Usually covers no existing slot fully
      return createDefaultOcclusion();

    case "anchored":
      // Partially covers nearby slots but doesn't fully cover
      return { covers: ["editor"], fullyCovers: [] };

    case "side-drawer":
      // May partially cover timeline on wide, fully cover on narrow
      return { covers: ["timeline"], fullyCovers: [] };

    case "status-line":
      return { covers: ["status"], fullyCovers: [] };

    default:
      return createDefaultOcclusion();
  }
}

/**
 * Resolve the interaction owner for a surface role.
 * - status: no interaction
 * - others: self (surface captures focus with its own key handler)
 */
function resolveInteractionOwner(role: SurfaceRole): SurfaceInteractionOwner {
  switch (role) {
    case "status":
      return "none";
    default:
      return "self";
  }
}

/**
 * Check if a role should block parent input.
 */
function isBlocking(_role: SurfaceRole, modality: SurfaceModality): boolean {
  return modality === "blocking" || modality === "modal";
}
