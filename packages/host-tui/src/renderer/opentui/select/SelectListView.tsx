// ============================================================================
// SelectListView — renders a selectable list from items with visible window
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";
import { computeSelectorLayout } from "./selector-layout.js";

export interface SelectListViewProps<T = unknown> {
  items: SelectItem<T>[];
  selectedIndex: number;
  filter?: string;
  showFilter?: boolean;
  showDescriptions?: boolean;
  maxHeight?: number;
  onSelect: (index: number, item: SelectItem<T>) => void;
  onFilterChange?: (value: string) => void;
}

export function SelectListView<T = unknown>(props: SelectListViewProps<T>) {
  const theme = useTheme();
  const {
    items,
    selectedIndex,
    filter = "",
    showFilter = false,
    showDescriptions = true,
    maxHeight = 12,
    onSelect,
    onFilterChange,
  } = props;

  const layout = computeSelectorLayout(items.length, maxHeight + 3, showFilter, 80);

  // Compute visible window
  const visibleStart = Math.max(0, selectedIndex - Math.floor(layout.visibleListRows / 2));
  const visibleItems = items.slice(visibleStart, visibleStart + layout.visibleListRows);

  return (
    <box flexDirection="column">
      {/* Filter row */}
      {showFilter && (
        <box height={1} paddingBottom={1}>
          <text fg={theme.color("text.muted")}>Filter: </text>
          <input
            value={filter}
            placeholder="Type to filter..."
            onInput={(value: string) => onFilterChange?.(value)}
          />
        </box>
      )}

      {/* List items */}
      {visibleItems.length > 0 ? (
        visibleItems.map((item, i) => {
          const actualIndex = visibleStart + i;
          const isSelected = actualIndex === selectedIndex;
          const prefix = isSelected ? "> " : "  ";

          return (
            <box
              flexDirection="row"
              height={1}
            >
              <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
                {prefix}{item.label}
              </text>
              {showDescriptions && item.description && (
                <text fg={theme.color("text.dim")}> — {item.description}</text>
              )}
              {item.badge && (
                <text fg={theme.color("ok")}> [{item.badge}]</text>
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
      {layout.showScrollCounter && (
        <box height={1}>
          <text fg={theme.color("text.dim")}>
            {visibleStart + 1}–{Math.min(visibleStart + layout.visibleListRows, items.length)} of{" "}
            {items.length}
          </text>
        </box>
      )}
    </box>
  );
}
