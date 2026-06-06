// ============================================================================
// Selector controller + types — shared selector contract
// ============================================================================

import type { KeyEvent } from "../../../focus/types.js";

export interface TextSegment {
  text: string;
  /** Theme token path for color, e.g. "text.accent", "text.muted" */
  color?: string;
}

export interface SelectItem<T = unknown> {
  id: string;
  label: string;
  /** Rich text segments — when present, rendered with per-segment colors instead of plain label */
  segments?: TextSegment[];
  description?: string;
  value: T;
  disabled?: boolean;
  badge?: string;
  /** Second-line metadata (e.g. "3 msgs · 2h"). When set, row renders as two lines. */
  meta?: string;
}

export interface SelectorConfig<T = unknown> {
  id: string;
  title: string;
  items: SelectItem<T>[];
  filterable?: boolean;
  onConfirm: (item: SelectItem<T>) => void;
  onCancel: () => void;
}

export interface SelectorLayout {
  maxListRows: number;
  visibleListRows: number;
  totalItems: number;
  showFilter: boolean;
  showDescriptions: boolean;
  showScrollCounter: boolean;
}

export class SelectorController<T = unknown> {
  id: string;
  title: string;
  private _items: SelectItem<T>[];
  private _query = "";
  private _selectedIndex = 0;
  private _filterable: boolean;
  private _onConfirm: (item: SelectItem<T>) => void;
  private _onCancel: () => void;

  constructor(config: SelectorConfig<T>) {
    this.id = config.id;
    this.title = config.title;
    this._items = config.items;
    this._filterable = config.filterable ?? false;
    this._onConfirm = config.onConfirm;
    this._onCancel = config.onCancel;
  }

  get query(): string {
    return this._query;
  }

  get selectedIndex(): number {
    return this._selectedIndex;
  }

  get filterable(): boolean {
    return this._filterable;
  }

  get visibleItems(): SelectItem<T>[] {
    if (!this._filterable || !this._query) return this._items;
    const q = this._query.toLowerCase();
    return this._items.filter(
      (item) => item.label.toLowerCase().includes(q) || item.description?.toLowerCase().includes(q),
    );
  }

  appendText(text: string): void {
    if (!this._filterable) return;
    this._query += text;
    this._selectedIndex = 0;
  }

  backspace(): void {
    if (!this._filterable) return;
    this._query = this._query.slice(0, -1);
    this._selectedIndex = 0;
  }

  move(delta: number): void {
    const max = Math.max(0, this.visibleItems.length - 1);
    this._selectedIndex = Math.max(0, Math.min(max, this._selectedIndex + delta));
  }

  page(delta: number): void {
    const pageSize = 5;
    this.move(delta * pageSize);
  }

  confirm(): void {
    const items = this.visibleItems;
    const idx = Math.min(this._selectedIndex, items.length - 1);
    const item = items[idx];
    if (item && !item.disabled) {
      this._onConfirm(item);
    }
  }

  cancel(): void {
    this._onCancel();
  }

  handleKey(event: KeyEvent): boolean {
    if (event.name === "up") {
      this.move(-1);
      return true;
    }
    if (event.name === "down") {
      this.move(1);
      return true;
    }
    if (event.name === "pageup") {
      this.page(-1);
      return true;
    }
    if (event.name === "pagedown") {
      this.page(1);
      return true;
    }
    if (event.name === "home") {
      this._selectedIndex = 0;
      return true;
    }
    if (event.name === "end") {
      this._selectedIndex = Math.max(0, this.visibleItems.length - 1);
      return true;
    }
    if (event.name === "enter" || event.name === "return") {
      this.confirm();
      return true;
    }
    if (event.name === "escape") {
      this.cancel();
      return true;
    }
    if (event.name === "backspace") {
      this.backspace();
      return true;
    }
    if (event.char && event.char.length === 1 && event.char >= " ") {
      this.appendText(event.char);
      return true;
    }
    return false;
  }
}
