// ============================================================================
// Surface resolver — mount strategy resolution from role, viewport, content
// ============================================================================

import type {
  SurfaceContext,
  SurfaceInteractionOwner,
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
  const mount = resolveMount(request, context);
  const occlusion = deriveOcclusion(mount, request, context);
  const zIndex = parentZIndex + 10; // Child surfaces are higher

  return {
    id: nextId(),
    mount,
    role: request.role,
    zIndex,
    parentId: request.parentId,
    anchorId: request.anchorId,
    targetSlot: request.targetSlot,
    insertAfterSlot: resolveInsertAfterSlot(request, mount),
    occlusion,
    interactionOwner: resolveInteractionOwner(request.role),
    blocking: isBlocking(request.role),
    data: request.data,
  };
}

/**
 * Resolve the mount strategy based on role, preference, viewport, and content size.
 */
function resolveMount(request: SurfaceRequest, context: SurfaceContext): SurfaceMount {
  const { viewportWidth } = context;
  const isNarrow = viewportWidth < 80;
  const isWide = viewportWidth >= 120;

  // Respect explicit preference when compatible
  if (request.preferredMount) {
    // Status and autocomplete have preferred mounts
    if (request.preferredMount === "anchored" && request.role === "autocomplete") {
      return "anchored";
    }
    if (request.preferredMount === "status-line" && request.role === "status") {
      return "status-line";
    }
    // For selectors and menus, honor side-drawer on wide terminals, fall back to insert-between on narrow
    if (
      request.preferredMount === "side-drawer" &&
      (request.role === "selector" || request.role === "menu")
    ) {
      if (isNarrow) return "replace-slot";
      return "side-drawer";
    }
    // Honor replace-slot for any role
    if (request.preferredMount === "replace-slot") {
      return "replace-slot";
    }
    // Honor insert-between for any role
    if (request.preferredMount === "insert-between") {
      return "insert-between";
    }
  }

  switch (request.role) {
    case "autocomplete":
      return "anchored";

    case "selector":
    case "menu":
      if (request.contentSize === "large") {
        if (isNarrow) return "replace-slot";
        if (isWide) return "side-drawer";
        return "insert-between";
      }
      if (isNarrow) return "replace-slot";
      return "insert-between";

    case "form":
      return isNarrow ? "replace-slot" : "insert-between";

    case "confirm":
      if (request.destructive) return "replace-slot";
      return "insert-between";

    case "status":
      return "status-line";

    default:
      return "insert-between";
  }
}

/**
 * Determine the insertAfterSlot for insert-between mounts.
 */
function resolveInsertAfterSlot(
  request: SurfaceRequest,
  mount: SurfaceMount,
): SurfaceSlot | undefined {
  if (mount !== "insert-between") return undefined;

  switch (request.targetSlot) {
    case "editor":
      return "timeline";
    case "timeline":
      return "status";
    case "status":
      return "editor";
    default:
      return "timeline";
  }
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
      const target = request.targetSlot ?? "app";
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
 * - autocomplete, form: interaction owned by anchor (editor stays focused)
 * - status: no interaction
 * - others: self (surface captures focus)
 */
function resolveInteractionOwner(role: SurfaceRole): SurfaceInteractionOwner {
  switch (role) {
    case "autocomplete":
      return "anchor";
    case "status":
      return "none";
    default:
      return "self";
  }
}

/**
 * Check if a role should block parent input.
 */
function isBlocking(role: SurfaceRole): boolean {
  switch (role) {
    case "selector":
    case "menu":
    case "form":
    case "confirm":
      return true;
    case "autocomplete":
    case "status":
      return false;
  }
}
