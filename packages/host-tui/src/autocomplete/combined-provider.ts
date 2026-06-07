// ============================================================================
// CombinedAutocompleteProvider — tries providers in order, returns first match
// ============================================================================

import type { AutocompleteItem, AutocompleteProvider, AutocompleteSuggestions } from "./types.js";

export class CombinedAutocompleteProvider implements AutocompleteProvider {
  private providers: AutocompleteProvider[];
  private providerMap: Map<string, AutocompleteProvider> = new Map();

  constructor(providers: Array<{ id: string; provider: AutocompleteProvider }>) {
    this.providers = providers.map((p) => p.provider);
    for (const { id, provider } of providers) {
      this.providerMap.set(id, provider);
    }
  }

  async getSuggestions(
    input: string,
    cursor: number,
    options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null> {
    for (const [_pid, prov] of this.providerMap.entries()) {
      try {
        const result = await prov.getSuggestions(input, cursor, options);
        if (result && result.items.length > 0) return result;
      } catch {
        // Provider failed, try next
      }
    }
    return null;
  }

  applyCompletion(
    input: string,
    cursor: number,
    item: AutocompleteItem,
    prefix: string,
  ): { input: string; cursor: number } {
    // Route by item's providerId, fallback to trying all
    if (item.providerId) {
      const provider = this.providerMap.get(item.providerId);
      if (provider) {
        try {
          return provider.applyCompletion(input, cursor, item, prefix);
        } catch {
          // fall through
        }
      }
    }
    // Legacy fallback: try all providers
    for (const provider of this.providers) {
      try {
        const result = provider.applyCompletion(input, cursor, item, prefix);
        if (result.input !== input) return result;
      } catch {
        // Try next
      }
    }
    return { input, cursor };
  }
}
