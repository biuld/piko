// ============================================================================
// EditorAutocompleteState — local autocomplete state types
// ============================================================================

import type { AutocompleteItem } from "../autocomplete/types.js";

export interface EditorAutocompleteState {
  visible: boolean;
  loading: boolean;
  query: string;
  providerId: string;
  prefix: string;
  items: AutocompleteItem[];
  selectedIndex: number;
}

export function createEmptyAutocompleteState(): EditorAutocompleteState {
  return {
    visible: false,
    loading: false,
    query: "",
    providerId: "",
    prefix: "",
    items: [],
    selectedIndex: 0,
  };
}

export interface AutocompleteApplyResult {
  input: string;
  cursor: number;
}
