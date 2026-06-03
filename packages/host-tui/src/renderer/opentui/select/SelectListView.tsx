// ============================================================================
// SelectListView — renders a selectable list from items with visible window
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { SelectItem } from "./selector-controller.js";
import { computeSelectorLayout } from "./selector-layout.js";
import { getSelectableListWindow } from "../../../surfaces/interactions/selectable-list.js";

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

  const maxHeight = () => props.maxHeight ?? 12;
  const showFilter = () => props.showFilter ?? false;
  const showDescriptions = () => props.showDescriptions ?? true;
  const layout = () =>
    computeSelectorLayout(props.items.length, maxHeight() + 3, showFilter(), 80);
  const visibleWindow = () =>
    getSelectableListWindow(
      props.items,
      props.selectedIndex,
      layout().visibleListRows,
    );
  const visibleStart = () => visibleWindow().start;
  const visibleItems = () => visibleWindow().rows;

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
          const prefix = isSelected ? "> " : "  ";

          return (
            <box
              flexDirection="row"
              height={1}
            >
              <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
                {prefix}{item.label}
              </text>
              {showDescriptions() && item.description && (
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
      {layout().showScrollCounter && (
        <box height={1}>
          <text fg={theme.color("text.dim")}>
            {visibleStart() + 1}–{Math.min(visibleStart() + layout().visibleListRows, props.items.length)} of{" "}
            {props.items.length}
          </text>
        </box>
      )}
    </box>
  );
}
