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

export type SelectableListScrollPolicy = "center" | "edge";

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
  isSelectableIndex?: (index: number) => boolean,
): SelectableListState {
  const selectedIndex = isSelectableIndex
    ? moveToSelectableIndex(state.selectedIndex, total, delta, isSelectableIndex)
    : clampListIndex(state.selectedIndex + delta, total);
  return {
    ...state,
    selectedIndex,
  };
}

export function pageListSelection(
  state: SelectableListState,
  total: number,
  delta: number,
  pageSize = 5,
  isSelectableIndex?: (index: number) => boolean,
): SelectableListState {
  return moveListSelection(state, total, delta * pageSize, isSelectableIndex);
}

export function homeListSelection(state: SelectableListState): SelectableListState {
  return { ...state, selectedIndex: 0 };
}

export function endListSelection(state: SelectableListState, total: number): SelectableListState {
  return { ...state, selectedIndex: clampListIndex(total - 1, total) };
}

export function firstSelectableIndex(
  total: number,
  isSelectableIndex?: (index: number) => boolean,
): number {
  if (total <= 0) return 0;
  if (!isSelectableIndex) return 0;
  for (let index = 0; index < total; index++) {
    if (isSelectableIndex(index)) return index;
  }
  return 0;
}

export function lastSelectableIndex(
  total: number,
  isSelectableIndex?: (index: number) => boolean,
): number {
  if (total <= 0) return 0;
  if (!isSelectableIndex) return clampListIndex(total - 1, total);
  for (let index = total - 1; index >= 0; index--) {
    if (isSelectableIndex(index)) return index;
  }
  return clampListIndex(total - 1, total);
}

export function nearestSelectableIndex(
  index: number,
  total: number,
  isSelectableIndex?: (index: number) => boolean,
): number {
  if (total <= 0) return 0;
  const clamped = clampListIndex(index, total);
  if (!isSelectableIndex || isSelectableIndex(clamped)) return clamped;

  for (let distance = 1; distance < total; distance++) {
    const before = clamped - distance;
    if (before >= 0 && isSelectableIndex(before)) return before;

    const after = clamped + distance;
    if (after < total && isSelectableIndex(after)) return after;
  }

  return clamped;
}

function moveToSelectableIndex(
  selectedIndex: number,
  total: number,
  delta: number,
  isSelectableIndex: (index: number) => boolean,
): number {
  if (total <= 0 || delta === 0) return clampListIndex(selectedIndex, total);

  const direction = delta > 0 ? 1 : -1;
  let remaining = Math.abs(delta);
  let index = nearestSelectableIndex(selectedIndex, total, isSelectableIndex);

  while (remaining > 0) {
    let next = index + direction;
    while (next >= 0 && next < total && !isSelectableIndex(next)) {
      next += direction;
    }
    if (next < 0 || next >= total) return index;
    index = next;
    remaining--;
  }

  return index;
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
  scrollPolicy: SelectableListScrollPolicy = "center",
): SelectableListWindow<T> {
  const visibleRows = Math.max(1, maxRows);
  const clamped = clampListIndex(selectedIndex, items.length);
  const maxStart = Math.max(0, items.length - visibleRows);
  const start =
    scrollPolicy === "edge"
      ? Math.max(0, Math.min(Math.floor(clamped / visibleRows) * visibleRows, maxStart))
      : Math.max(0, Math.min(clamped - Math.floor(visibleRows / 2), maxStart));
  return {
    start,
    rows: items.slice(start, start + visibleRows),
  };
}

export interface SelectableListKeyOptions {
  total: number;
  filterable?: boolean;
  isSelectableIndex?: (index: number) => boolean;
}

export function handleSelectableListKey(
  state: SelectableListState,
  event: KeyEvent,
  options: SelectableListKeyOptions,
): SelectableListState | undefined {
  if (event.name === "up") {
    return moveListSelection(state, options.total, -1, options.isSelectableIndex);
  }
  if (event.name === "down") {
    return moveListSelection(state, options.total, 1, options.isSelectableIndex);
  }
  if (event.name === "pageup") {
    return pageListSelection(state, options.total, -1, 5, options.isSelectableIndex);
  }
  if (event.name === "pagedown") {
    return pageListSelection(state, options.total, 1, 5, options.isSelectableIndex);
  }
  if (event.name === "home") {
    return {
      ...homeListSelection(state),
      selectedIndex: firstSelectableIndex(options.total, options.isSelectableIndex),
    };
  }
  if (event.name === "end") {
    return {
      ...endListSelection(state, options.total),
      selectedIndex: lastSelectableIndex(options.total, options.isSelectableIndex),
    };
  }
  if (!options.filterable) return undefined;
  if (event.name === "backspace") return backspaceListQuery(state);
  if (event.char && event.char.length === 1 && event.char >= " ") {
    return appendListQuery(state, event.char);
  }
  return undefined;
}
