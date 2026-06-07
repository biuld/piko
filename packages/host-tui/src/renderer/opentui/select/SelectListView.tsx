// ============================================================================
// SelectListView — renders a selectable list from items with visible window.
// Supports text truncation to fit terminal width.
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";
import { clampListIndex, type SelectableListScrollPolicy } from "../../../surfaces/interactions/selectable-list.js";
import { truncateToWidth, visibleWidth } from "../../../layout/measure.js";

export interface SelectListViewProps<T = unknown> {
  items: SelectItem<T>[];
  selectedIndex: number;
  /** Terminal/surface width in columns (for description truncation) */
  width?: number;
  filter?: string;
  showFilter?: boolean;
  showDescriptions?: boolean;
  maxHeight?: number;
  scrollPolicy?: SelectableListScrollPolicy;
  /**
   * Optional item height override in terminal lines, including itemSpacing.
   * By default, SelectListView derives this from the item model.
   */
  rowHeight?: number;
  /** Blank terminal lines inserted between visible items. */
  itemSpacing?: number;
  onSelect: (index: number, item: SelectItem<T>) => void;
  onFilterChange?: (value: string) => void;
}

interface HeightWindow<T> {
  start: number;
  rows: T[];
}

function getListWindowByHeight<T>(
  items: readonly T[],
  selectedIndex: number,
  maxHeight: number,
  scrollPolicy: SelectableListScrollPolicy,
  getItemHeight: (item: T) => number,
  itemSpacing: number,
): HeightWindow<T> {
  if (items.length === 0) return { start: 0, rows: [] };

  const budget = Math.max(1, maxHeight);
  const selected = clampListIndex(selectedIndex, items.length);

  function heightOf(start: number, endExclusive: number): number {
    let height = 0;
    for (let index = start; index < endExclusive; index++) {
      if (index > start) height += itemSpacing;
      height += Math.max(1, getItemHeight(items[index]));
    }
    return height;
  }

  function countFrom(start: number): number {
    let height = 0;
    let count = 0;
    for (let index = start; index < items.length; index++) {
      const nextHeight =
        height + (count > 0 ? itemSpacing : 0) + Math.max(1, getItemHeight(items[index]));
      if (count > 0 && nextHeight > budget) break;
      height = nextHeight;
      count++;
      if (height >= budget) break;
    }
    return Math.max(1, count);
  }

  function endFor(start: number): number {
    return Math.min(items.length, start + countFrom(start));
  }

  if (scrollPolicy === "edge") {
    let start = 0;
    while (start < items.length) {
      const end = endFor(start);
      if (selected < end || end >= items.length) {
        return { start, rows: items.slice(start, end) };
      }
      start = end;
    }
    return { start: items.length - 1, rows: items.slice(items.length - 1) };
  }

  let start = selected;
  let end = selected + 1;
  let height = Math.max(1, getItemHeight(items[selected]));

  while (height < budget && (start > 0 || end < items.length)) {
    const beforeCount = selected - start;
    const afterCount = end - selected - 1;
    const preferBefore = beforeCount <= afterCount;
    const candidates = preferBefore ? (["before", "after"] as const) : (["after", "before"] as const);
    let added = false;

    for (const candidate of candidates) {
      if (candidate === "before" && start > 0) {
        const nextHeight = height + itemSpacing + Math.max(1, getItemHeight(items[start - 1]));
        if (nextHeight <= budget) {
          start--;
          height = nextHeight;
          added = true;
          break;
        }
      }
      if (candidate === "after" && end < items.length) {
        const nextHeight = height + itemSpacing + Math.max(1, getItemHeight(items[end]));
        if (nextHeight <= budget) {
          end++;
          height = nextHeight;
          added = true;
          break;
        }
      }
    }

    if (!added) break;
  }

  return { start, rows: items.slice(start, end) };
}

export function SelectListView<T = unknown>(props: SelectListViewProps<T>) {
  const theme = useTheme();

  // When embedded in a panel, maxHeight should be passed from props
  // For now we fallback to 15 if not provided
  const maxHeight = () => props.maxHeight ?? 15;
  const showFilter = () => props.showFilter ?? false;
  const showDescriptions = () => props.showDescriptions ?? true;
  const terminalWidth = () => Math.max(1, (props.width ?? 80) - 4);
  const itemSpacing = () => Math.max(0, props.itemSpacing ?? 0);
  const scrollPolicy = () => props.scrollPolicy ?? "center";
  const listHeight = () => Math.max(1, maxHeight() - (showFilter() ? 1 : 0));
  const visibleWindow = () => getListWindowByHeight(
    props.items,
    props.selectedIndex,
    listHeight(),
    scrollPolicy(),
    itemBaseHeight,
    itemSpacing(),
  );
  const visibleStart = () => visibleWindow().start;
  const visibleItems = () => visibleWindow().rows;
  const rowItems = () => visibleItems();
  const rowStart = () => visibleStart();

  // ==========================================================================
  // Row rendering
  // ==========================================================================

  interface RowParts {
    labelLeft: string;
    desc: string | null;
  }

  function formatRow(
    item: SelectItem<T>,
    isSelected: boolean,
    width: number,
  ): RowParts {
    const prefix = isSelected ? "> " : "  ";
    const badge = item.badge ? ` [${item.badge}]` : "";

    const reserved = 2 + (showDescriptions() ? 3 : 0) + visibleWidth(badge) + 2;
    const available = Math.max(10, width - reserved);
    const labelMax = Math.max(6, Math.floor(available * 0.45));
    const truncatedLabel = truncateToWidth(item.label, labelMax);

    if (!item.description) {
      return { labelLeft: prefix + truncatedLabel + badge, desc: null };
    }

    const labelUsed = visibleWidth(truncatedLabel);
    const descMax = Math.max(4, available - labelUsed);
    const truncatedDesc = descMax < 6
      ? null
      : truncateToWidth(item.description, descMax);

    return { labelLeft: prefix + truncatedLabel, desc: truncatedDesc };
  }

  // Render a meta row: line 1 = title, line 2 = meta info (dim).
  function renderMetaRow(item: SelectItem<T>, isSelected: boolean) {
    const prefix = isSelected ? "> " : "  ";
    const truncatedTitle = truncateToWidth(item.label, terminalWidth() - visibleWidth(prefix) - 2, "…");
    const highlightBg = isSelected ? theme.color("surface.selected") : undefined;

    return (
      <box flexDirection="column">
        <box flexDirection="row" width={terminalWidth()} height={1} backgroundColor={highlightBg}>
          <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
            {prefix + truncatedTitle}
          </text>
        </box>
        <box flexDirection="row" height={1}>
          <text fg={theme.color("text.dim")}>
            {"  " + item.meta}
          </text>
        </box>
      </box>
    );
  }

  function renderRow(item: SelectItem<T>, actualIndex: number) {
    const isSelected = actualIndex === props.selectedIndex;

    if (item.meta) {
      return renderMetaRow(item, isSelected);
    }

    const row = formatRow(item, isSelected, terminalWidth());
    const hasSegments = item.segments && item.segments.length > 0;

    return (
      <box
        flexDirection="row"
        height={1}
        backgroundColor={isSelected ? theme.color("surface.selected") : undefined}
      >
        {hasSegments ? (
          <box flexDirection="row">
            {item.segments!.map((seg) => (
              <text
                fg={
                  seg.color
                    ? theme.color(seg.color)
                    : isSelected
                      ? theme.color("text.accent")
                      : theme.color("text.primary")
                }
              >
                {seg.text}
              </text>
            ))}
          </box>
        ) : (
          <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
            {row.labelLeft}
          </text>
        )}
        {row.desc ? <text fg={theme.color("text.dim")}> — {row.desc}</text> : null}
      </box>
    );
  }

  function itemContentHeight(item: SelectItem<T>): number {
    return item.meta ? 2 : 1;
  }

  function itemBaseHeight(item: SelectItem<T>): number {
    const overrideContentHeight =
      props.rowHeight === undefined ? 0 : Math.max(1, props.rowHeight - itemSpacing());
    return Math.max(itemContentHeight(item), overrideContentHeight);
  }

  function renderItemBlock(item: SelectItem<T>, actualIndex: number, includeSpacing: boolean) {
    const contentHeight = itemContentHeight(item);
    const blockHeight = itemBaseHeight(item) + (includeSpacing ? itemSpacing() : 0);
    const spacerHeight = Math.max(0, blockHeight - contentHeight);

    return (
      <box flexDirection="column" height={blockHeight} flexShrink={0} overflow="hidden">
        {renderRow(item, actualIndex)}
        {spacerHeight > 0 ? (
          <box height={spacerHeight} flexShrink={0} />
        ) : null}
      </box>
    );
  }

  return (
    <box flexDirection="column">
      {/* Filter row */}
      {showFilter() && (
        <box height={1} paddingBottom={1}>
          <text fg={theme.color("text.muted")}>Filter: </text>
          <input
            value={props.filter ?? ""}
            placeholder="Type to filter..."
            onInput={(value: string) => props.onFilterChange?.(value)}
          />
        </box>
      )}

      <box flexDirection="column" height={listHeight()} flexShrink={0}>
        {/* List items */}
        {rowItems().length > 0 ? (
          rowItems().map((item, i) => {
            return renderItemBlock(item, rowStart() + i, i < rowItems().length - 1);
          })
        ) : (
          <box height={1}>
            <text fg={theme.color("text.muted")}>No items found</text>
          </box>
        )}
      </box>


    </box>
  );
}
