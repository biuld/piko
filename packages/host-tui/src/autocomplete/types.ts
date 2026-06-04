// ============================================================================
// Autocomplete types — provider API, suggestions, completion application
// ============================================================================

export interface AutocompleteItem {
  value: string;
  label: string;
  description?: string;
  /** Which provider produced this item (for accept routing) */
  providerId?: string;
}

export interface AutocompleteSuggestions {
  items: AutocompleteItem[];
  /** The matched prefix that triggered these suggestions (e.g., "/mod") */
  prefix: string;
  /** Which provider produced these items (for correct accept/submit routing) */
  providerId: string;
}

export interface AutocompleteProvider {
  /**
   * Get suggestions for the given input at cursor position.
   * Returns null if this provider has no suggestions.
   */
  getSuggestions(
    input: string,
    cursor: number,
    options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null>;

  /**
   * Apply a completion to the input, returning the new input text and cursor position.
   */
  applyCompletion(
    input: string,
    cursor: number,
    item: AutocompleteItem,
    prefix: string,
  ): { input: string; cursor: number };
}
