// ============================================================================
// Editor subsystem — public API
// ============================================================================

export type { EditorAction } from "./editor-actions.js";
export { EditorAutocompleteController } from "./editor-autocomplete-controller.js";
export type {
  AutocompleteApplyResult,
  EditorAutocompleteState,
} from "./editor-autocomplete-state.js";
export { createEmptyAutocompleteState } from "./editor-autocomplete-state.js";
