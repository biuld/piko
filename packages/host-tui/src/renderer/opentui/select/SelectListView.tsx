// ============================================================================
// SelectListView — renders a selectable list from items with visible window.
// Supports text truncation to fit terminal width.
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";
import {
  getSelectableListWindow,
  type SelectableListScrollPolicy,
} from "../../../surfaces/interactions/selectable-list.js";
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
  /** Row height in terminal lines. Default 1. Use 2+ when items have meta. */
  rowHeight?: number;
  onSelect: (index: number, item: SelectItem<T>) => void;
  onFilterChange?: (value: string) => void;
}

export function SelectListView<T = unknown>(props: SelectListViewProps<T>) {
  const theme = useTheme();

  // When embedded in a panel, maxHeight should be passed from props
  // For now we fallback to 15 if not provided
  const maxHeight = () => props.maxHeight ?? 15;
  const showFilter = () => props.showFilter ?? false;
  const showDescriptions = () => props.showDescriptions ?? true;
  const terminalWidth = () => props.width ?? 80;
  const rowHeight = () => props.rowHeight ?? 1;
  const scrollPolicy = () => props.scrollPolicy ?? "center";
  const visibleListRows = () => {
    const reservedRows = showFilter() ? 1 : 0;
    const maxVisible = Math.floor((maxHeight() - reservedRows) / rowHeight());
    return Math.max(1, Math.min(props.items.length, maxVisible));
  };
  const visibleWindow = () =>
    getSelectableListWindow(props.items, props.selectedIndex, visibleListRows(), scrollPolicy());
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
        {/* Highlighted area: title + meta only */}
        <box flexDirection="column" width={terminalWidth()} backgroundColor={highlightBg}>
          <box flexDirection="row" height={1}>
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
        {/* Spacer line between sessions — not highlighted */}
        <box height={1} />
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
        {item.badge ? <text fg={theme.color("text.success")}> [{item.badge}]</text> : null}
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

      {/* List items */}
      {rowItems().length > 0 ? (
        rowItems().map((item, i) => {
          return renderRow(item, rowStart() + i);
        })
      ) : (
        <box height={1}>
          <text fg={theme.color("text.muted")}>No items found</text>
        </box>
      )}


    </box>
  );
}
