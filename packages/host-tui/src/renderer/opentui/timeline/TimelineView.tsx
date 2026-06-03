// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
// ============================================================================

import type { ScrollBoxRenderable } from "@opentui/core";
import { createEffect } from "solid-js";
import type { TimelineItem, TimelineLayout } from "../../../timeline/types.js";
import { TimelineItemView } from "./TimelineItemView.js";
import { TimelineSeparator } from "./TimelineSeparator.js";
import { LatestIndicator } from "./LatestIndicator.js";

export interface TimelineViewProps {
  items: TimelineItem[];
  layout: TimelineLayout;
  pendingNewItems: number;
  expandedItemIds: Set<string>;
  collapsedToolCallIds: Set<string>;
  stickyBottom: boolean;
}

export function TimelineView(props: TimelineViewProps) {
  const {
    items,
    layout,
    pendingNewItems,
    expandedItemIds,
    collapsedToolCallIds,
    stickyBottom,
  } = props;

  let scrollboxEl: ScrollBoxRenderable | undefined;

  // When stickyBottom re-engages (false → true), explicitly scroll to bottom.
  // The stickyScroll prop alone may not trigger immediate scroll if the
  // internal _hasManualScroll flag is still set.
  createEffect(() => {
    if (stickyBottom && scrollboxEl) {
      // Schedule scroll on next microtask so the renderable is fully laid out
      queueMicrotask(() => {
        scrollboxEl?.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
      });
    }
  });

  return (
    <box flexDirection="column" flexGrow={1} overflow="hidden">
      <scrollbox
        ref={(el: ScrollBoxRenderable) => { scrollboxEl = el; }}
        flexGrow={1}
        flexShrink={1}
        height="100%"
        stickyScroll={stickyBottom}
        stickyStart="bottom"
      >
        {items.map((item, i) => (
          <>
            {i > 0 && <TimelineSeparator />}
            <TimelineItemView
              item={item}
              layout={layout}
              isExpanded={expandedItemIds.has(item.id)}
              isCollapsed={collapsedToolCallIds.has(item.toolCallId ?? "")}
            />
          </>
        ))}
      </scrollbox>

      {pendingNewItems > 0 && (
        <LatestIndicator count={pendingNewItems} />
      )}
    </box>
  );
}
