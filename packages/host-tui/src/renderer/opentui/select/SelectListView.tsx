// ============================================================================
// SelectListView — renders a selectable list from items with visible window.
// Supports text truncation to fit terminal width.
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";
import { getSelectableListWindow } from "../../../surfaces/interactions/selectable-list.js";
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
  const showCounter = () => {
    const baseRows = Math.max(1, maxHeight() - (showFilter() ? 1 : 0));
    return props.items.length > baseRows;
  };
  const visibleListRows = () => {
    const reservedRows = (showFilter() ? 1 : 0) + (showCounter() ? 1 : 0);
    return Math.max(1, Math.min(props.items.length, maxHeight() - reservedRows));
  };
  const visibleWindow = () =>
    getSelectableListWindow(props.items, props.selectedIndex, visibleListRows());
  const visibleStart = () => visibleWindow().start;
  const visibleItems = () => visibleWindow().rows;

  // Format a single row with truncation to fit terminal width.
  // Layout: "  label — description [badge]"
  function formatRow(
    item: SelectItem<T>,
    isSelected: boolean,
    width: number,
  ): { label: string; desc: string | null } {
    const prefix = isSelected ? "> " : "  ";
    const badge = item.badge ? ` [${item.badge}]` : "";
    // Reserve: prefix (2) + separator (3 for " — ") + inner padding (2)
    const reserved = 2 + (showDescriptions() ? 3 : 0) + visibleWidth(badge) + 2;
    const available = Math.max(10, width - reserved);

    // Allocate ~45% to label, rest to description
    const labelMax = Math.max(6, Math.floor(available * 0.45));
    const truncatedLabel = truncateToWidth(item.label, labelMax);
    const labelPart = prefix + truncatedLabel;

    if (!showDescriptions() || !item.description) {
      return { label: labelPart + badge, desc: null };
    }

    const labelUsed = visibleWidth(truncatedLabel);
    const descMax = Math.max(4, available - labelUsed);
    const truncatedDesc = descMax < 6
      ? null
      : truncateToWidth(item.description, descMax);

    return {
      label: labelPart,
      desc: truncatedDesc,
    };
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
      {visibleItems().length > 0 ? (
        visibleItems().map((item, i) => {
          const actualIndex = visibleStart() + i;
          const isSelected = actualIndex === props.selectedIndex;
          const row = formatRow(item, isSelected, terminalWidth());

          return (
            <box
              flexDirection="row"
              height={1}
            >
              <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
                {row.label}
              </text>
              {row.desc && (
                <text fg={theme.color("text.dim")}> — {row.desc}</text>
              )}
              {item.badge && (
                <text fg={theme.color("text.success")}> [{item.badge}]</text>
              )}
            </box>
          );
        })
      ) : (
        <box height={1}>
          <text fg={theme.color("text.muted")}>No items found</text>
        </box>
      )}

      {/* Scroll counter */}
      {showCounter() && (
        <box height={1}>
          <text fg={theme.color("text.dim")}>
            {visibleStart() + 1}–{Math.min(visibleStart() + visibleListRows(), props.items.length)} of {props.items.length}
          </text>
        </box>
      )}
    </box>
  );
}
