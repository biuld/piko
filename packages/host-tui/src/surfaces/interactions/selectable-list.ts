// ============================================================================
// Selectable list interaction primitive
// ============================================================================

import type { KeyEvent } from "../../focus/types.js";

export interface SelectableListItem<T = unknown> {
  id: string;
  label: string;
  description?: string;
  value: T;
  disabled?: boolean;
  badge?: string;
}

export interface SelectableListState {
  query: string;
  selectedIndex: number;
}

export interface SelectableListWindow<T = unknown> {
  start: number;
  rows: T[];
}

export function createSelectableListState(): SelectableListState {
  return { query: "", selectedIndex: 0 };
}

export function clampListIndex(index: number, total: number): number {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(index, total - 1));
}

export function moveListSelection(
  state: SelectableListState,
  total: number,
  delta: number,
): SelectableListState {
  return {
    ...state,
    selectedIndex: clampListIndex(state.selectedIndex + delta, total),
  };
}

export function pageListSelection(
  state: SelectableListState,
  total: number,
  delta: number,
  pageSize = 5,
): SelectableListState {
  return moveListSelection(state, total, delta * pageSize);
}

export function homeListSelection(state: SelectableListState): SelectableListState {
  return { ...state, selectedIndex: 0 };
}

export function endListSelection(state: SelectableListState, total: number): SelectableListState {
  return { ...state, selectedIndex: clampListIndex(total - 1, total) };
}

export function setListQuery(state: SelectableListState, query: string): SelectableListState {
  return { ...state, query, selectedIndex: 0 };
}

export function appendListQuery(state: SelectableListState, text: string): SelectableListState {
  return setListQuery(state, state.query + text);
}

export function backspaceListQuery(state: SelectableListState): SelectableListState {
  return setListQuery(state, state.query.slice(0, -1));
}

export function filterSelectableItems<T>(
  items: readonly SelectableListItem<T>[],
  query: string,
): SelectableListItem<T>[] {
  const q = query.toLowerCase().trim();
  if (!q) return [...items];
  return items.filter(
    (item) => item.label.toLowerCase().includes(q) || item.description?.toLowerCase().includes(q),
  );
}

export function getSelectedItem<T>(
  items: readonly SelectableListItem<T>[],
  selectedIndex: number,
): SelectableListItem<T> | undefined {
  return items[clampListIndex(selectedIndex, items.length)];
}

export function getSelectableListWindow<T>(
  items: readonly T[],
  selectedIndex: number,
  maxRows: number,
): SelectableListWindow<T> {
  const visibleRows = Math.max(1, maxRows);
  const clamped = clampListIndex(selectedIndex, items.length);
  const start = Math.max(
    0,
    Math.min(clamped - Math.floor(visibleRows / 2), items.length - visibleRows),
  );
  return {
    start,
    rows: items.slice(start, start + visibleRows),
  };
}

export interface SelectableListKeyOptions {
  total: number;
  filterable?: boolean;
}

export function handleSelectableListKey(
  state: SelectableListState,
  event: KeyEvent,
  options: SelectableListKeyOptions,
): SelectableListState | undefined {
  if (event.name === "up") return moveListSelection(state, options.total, -1);
  if (event.name === "down") return moveListSelection(state, options.total, 1);
  if (event.name === "pageup") return pageListSelection(state, options.total, -1);
  if (event.name === "pagedown") return pageListSelection(state, options.total, 1);
  if (event.name === "home") return homeListSelection(state);
  if (event.name === "end") return endListSelection(state, options.total);
  if (!options.filterable) return undefined;
  if (event.name === "backspace") return backspaceListQuery(state);
  if (event.char && event.char.length === 1 && event.char >= " ") {
    return appendListQuery(state, event.char);
  }
  return undefined;
}
