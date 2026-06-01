// ============================================================================
// Command dispatcher — maps CommandId to ActionService + TuiStore side effects
// Used by both App (keyboard) and Editor (slash commands)
// ============================================================================

import type { ActionService } from "./action-service.js";
import type { CommandId } from "./keybinding-registry.js";
import type { TuiStore } from "./store.js";

/**
 * Dispatch a command ID to the appropriate action.
 * Single place where command IDs map to side effects.
 */
export function dispatchCommand(command: CommandId, svc: ActionService, store: TuiStore): void {
  switch (command) {
    case "openModel":
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      });
      break;
    case "openThinking":
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "thinking", isOpen: true, placement: "modal" },
      });
      break;
    case "openResume":
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "resume", isOpen: true, placement: "modal" },
      });
      break;
    case "openSettings":
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "settings", isOpen: true, placement: "modal" },
      });
      break;
    case "openLogin":
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "login", isOpen: true, placement: "modal" },
      });
      break;
    case "closeOverlay":
      store.dispatch({ type: "overlay_closed" });
      break;
    case "quit":
      svc.shutdown();
      break;
    case "abort":
      svc.abortRun();
      break;
    case "submit":
      // Editor handles submit directly; keyboard submit is no-op here
      break;
    default:
      // Unknown commands are ignored
      break;
  }
}
