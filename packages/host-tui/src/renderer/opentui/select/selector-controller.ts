// ============================================================================
// Selector types — shared selector contract
// ============================================================================

export interface SelectItem<T = unknown> {
  id: string;
  label: string;
  description?: string;
  value: T;
  disabled?: boolean;
  badge?: string;
}

export interface SelectorController<T = unknown> {
  id: string;
  title: string;
  items: () => SelectItem<T>[];
  selectedIndex: () => number;
  filter: () => string;
  setFilter: (value: string) => void;
  move: (delta: number) => void;
  page: (delta: number) => void;
  confirm: () => void;
  cancel: () => void;
}

export interface SelectorLayout {
  maxListRows: number;
  visibleListRows: number;
  totalItems: number;
  showFilter: boolean;
  showDescriptions: boolean;
  showScrollCounter: boolean;
}
