// ============================================================================
// EditorAutocompleteController — manages autocomplete lifecycle locally.
// No global store writes for items/query/selectedIndex.
// ============================================================================

import type { CombinedAutocompleteProvider } from "../autocomplete/combined-provider.js";
import type { AutocompleteItem } from "../autocomplete/types.js";
import type {
  AutocompleteApplyResult,
  EditorAutocompleteState,
} from "./editor-autocomplete-state.js";
import { createEmptyAutocompleteState } from "./editor-autocomplete-state.js";

export class EditorAutocompleteController {
  private provider: CombinedAutocompleteProvider;
  private _state: EditorAutocompleteState;
  private _onChange?: (state: EditorAutocompleteState) => void;
  private _abortController: AbortController | null = null;
  /** Callback to sync items to TuiController for interceptor access. */
  private _onItemsChange?: (items: AutocompleteItem[], prefix: string, providerId: string) => void;
  /**
   * Optional sync fallback provider for instant slash suggestions while
   * async provider loads. Items from this callback are merged into visibleItems
   * so that UI display and key behavior read the same source of truth.
   */
  private _getSyncFallback?: (input: string) => AutocompleteItem[];

  constructor(
    provider: CombinedAutocompleteProvider,
    onChange?: (state: EditorAutocompleteState) => void,
    onItemsChange?: (items: AutocompleteItem[], prefix: string, providerId: string) => void,
    getSyncFallback?: (input: string) => AutocompleteItem[],
  ) {
    this.provider = provider;
    this._state = createEmptyAutocompleteState();
    this._onChange = onChange;
    this._onItemsChange = onItemsChange;
    this._getSyncFallback = getSyncFallback;
  }

  get state(): EditorAutocompleteState {
    return this._state;
  }

  /**
   * Unified visible items: async results first, then sync fallback.
   * This is the single source of truth for both UI rendering and key behavior.
   */
  get visibleItems(): AutocompleteItem[] {
    if (this._state.items.length > 0) return this._state.items;
    if (this._state.loading && this._getSyncFallback) {
      const fallback = this._getSyncFallback(this._state.query);
      if (fallback.length > 0) return fallback;
    }
    return this._state.items;
  }

  private emit(): void {
    this._onChange?.({ ...this._state });
  }

  /**
   * Query the provider for suggestions. Cancels previous request.
   */
  async query(input: string, cursor: number): Promise<void> {
    // Cancel previous inflight request
    this._abortController?.abort();
    this._abortController = new AbortController();
    const signal = this._abortController.signal;

    const show = input.startsWith("/") || input.includes("@");
    if (!show) {
      this._state = createEmptyAutocompleteState();
      this.emit();
      this._onItemsChange?.([], "", "");
      return;
    }

    this._state = {
      ...this._state,
      visible: true,
      loading: true,
      query: input,
    };
    this.emit();

    try {
      const result = await this.provider.getSuggestions(input, cursor, {
        force: false,
        signal,
      });

      // Aborted by a newer query — do not mutate current state.
      // The newer request owns the current _state and will set loading: false
      // on its own completion.
      if (signal.aborted) return;

      if (result && result.items.length > 0) {
        this._state = {
          visible: true,
          loading: false,
          query: input,
          providerId: result.providerId ?? "",
          prefix: result.prefix ?? "",
          items: result.items,
          selectedIndex: Math.min(this._state.selectedIndex, result.items.length - 1),
        };
      } else {
        this._state = {
          ...this._state,
          visible: true,
          loading: false,
          query: input,
          items: [],
        };
      }

      this.emit();
      this._onItemsChange?.(this._state.items, this._state.prefix, this._state.providerId);
    } catch (_err) {
      // Aborted by a newer query — do not mutate state.
      if (signal.aborted) return;
      this._state = {
        ...this._state,
        loading: false,
        items: [],
      };
      this.emit();
      this._onItemsChange?.([], "", "");
    }
  }

  /** Move selection up or down. Clamped to visible items length. */
  move(delta: number): void {
    const items = this.visibleItems;
    const max = Math.max(0, items.length - 1);
    const next = Math.max(0, Math.min(max, this._state.selectedIndex + delta));
    if (next !== this._state.selectedIndex) {
      this._state = { ...this._state, selectedIndex: next };
      this.emit();
    }
  }

  /** Accept the currently selected item from visible items. Returns apply result or null. */
  accept(): AutocompleteApplyResult | null {
    if (!this._state.visible) return null;
    const items = this.visibleItems;
    if (items.length === 0) return null;
    const idx = Math.min(this._state.selectedIndex, items.length - 1);
    const item = items[idx];
    if (!item) return null;
    return this.provider.applyCompletion(
      this._state.query,
      this._state.query.length,
      item,
      item.providerId === "slash" ? this._state.query.trimStart() : this._state.prefix,
    );
  }

  /** Cancel autocomplete. Resets to empty state. */
  cancel(): void {
    this._abortController?.abort();
    this._state = createEmptyAutocompleteState();
    this.emit();
    this._onItemsChange?.([], "", "");
  }

  /** Get current selected item from visible items. */
  getSelectedItem(): AutocompleteItem | null {
    if (!this._state.visible) return null;
    const items = this.visibleItems;
    if (items.length === 0) return null;
    return items[Math.min(this._state.selectedIndex, items.length - 1)] ?? null;
  }

  /** Check if current provider is slash (for Enter routing). */
  isSlashProvider(): boolean {
    return (
      this._state.visible &&
      (this._state.providerId === "slash" || this._state.query.trimStart().startsWith("/"))
    );
  }

  dispose(): void {
    this.cancel();
  }
}
