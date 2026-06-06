// ============================================================================
// SelectListView — renders a selectable list from items with visible window.
// Supports text truncation to fit terminal width.
// ============================================================================

import type { ScrollBoxRenderable } from "@opentui/core";
import { createEffect, createSignal, onCleanup } from "solid-js";
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
  /** Render inside an OpenTUI scrollbox so mouse wheel/trackpad scrolling works. */
  scrollable?: boolean;
  onSelect: (index: number, item: SelectItem<T>) => void;
  onFilterChange?: (value: string) => void;
}

export function SelectListView<T = unknown>(props: SelectListViewProps<T>) {
  const theme = useTheme();
  const [scrollboxEl, setScrollboxEl] = createSignal<ScrollBoxRenderable | undefined>();
  let scrollRetryTimers: ReturnType<typeof setTimeout>[] = [];

  // When embedded in a panel, maxHeight should be passed from props
  // For now we fallback to 15 if not provided
  const maxHeight = () => props.maxHeight ?? 15;
  const showFilter = () => props.showFilter ?? false;
  const showDescriptions = () => props.showDescriptions ?? true;
  const scrollable = () => props.scrollable ?? true;
  const terminalWidth = () => props.width ?? 80;
  const visibleListRows = () => {
    const reservedRows = showFilter() ? 1 : 0;
    return Math.max(1, Math.min(props.items.length, maxHeight() - reservedRows));
  };
  const visibleWindow = () =>
    getSelectableListWindow(props.items, props.selectedIndex, visibleListRows());
  const visibleStart = () => visibleWindow().start;
  const visibleItems = () => visibleWindow().rows;
  const rowItems = () => (scrollable() ? props.items : visibleItems());
  const rowStart = () => (scrollable() ? 0 : visibleStart());

  function ensureSelectedVisible() {
    const el = scrollboxEl();
    if (!scrollable() || !el) return;
    const selectedIndex = props.selectedIndex;
    const viewportHeight = el.viewport.height || visibleListRows();
    const scrollTop = el.scrollTop;
    if (selectedIndex < scrollTop) {
      el.scrollTo({ x: 0, y: visibleStart() });
    } else if (selectedIndex >= scrollTop + viewportHeight) {
      el.scrollTo({ x: 0, y: visibleStart() });
    }
  }

  function scheduleEnsureSelectedVisible() {
    for (const timer of scrollRetryTimers) clearTimeout(timer);
    scrollRetryTimers = [];
    ensureSelectedVisible();
    // ScrollBox clamps before its content/viewport sizes settle. Retry across the next frames.
    for (const delay of [0, 16, 50, 100]) {
      scrollRetryTimers.push(setTimeout(ensureSelectedVisible, delay));
    }
  }

  createEffect(() => {
    props.selectedIndex;
    props.items.length;
    visibleListRows();
    scheduleEnsureSelectedVisible();
  });

  onCleanup(() => {
    for (const timer of scrollRetryTimers) clearTimeout(timer);
  });

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

  function renderRow(item: SelectItem<T>, actualIndex: number) {
    const isSelected = actualIndex === props.selectedIndex;
    const row = formatRow(item, isSelected, terminalWidth());
    const hasSegments = item.segments && item.segments.length > 0;

    return (
      <box
        flexDirection="row"
        height={1}
        backgroundColor={isSelected ? theme.color("surface.selected") : undefined}
      >
        {hasSegments ? (
          // Rich text: render each segment with its own color
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
          // Plain label: single color
          <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
            {row.label}
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
      {scrollable() ? (
        <scrollbox
          ref={(el: ScrollBoxRenderable) => {
            setScrollboxEl(el);
            queueMicrotask(scheduleEnsureSelectedVisible);
          }}
          height={visibleListRows()}
          flexShrink={1}
          stickyScroll={false}
        >
          {rowItems().length > 0 ? (
            rowItems().map((item, i) => {
              return renderRow(item, rowStart() + i);
            })
          ) : (
            <box height={1}>
              <text fg={theme.color("text.muted")}>No items found</text>
            </box>
          )}
        </scrollbox>
      ) : rowItems().length > 0 ? (
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
