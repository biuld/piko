// ============================================================================
// Focus — public API
// ============================================================================

export { FocusManager } from "./focus-manager.js";
export type { InputRouterOptions } from "./input-router.js";
export { InputRouter } from "./input-router.js";
export type { RawKeyEvent } from "./key-normalize.js";
export { normalizeKeyEvent, normalizeKeyName } from "./key-normalize.js";
export type {
  FocusNode,
  FocusOwner,
  FocusRegion,
  FocusResult,
  KeyEvent,
  TuiFocusState,
} from "./types.js";
