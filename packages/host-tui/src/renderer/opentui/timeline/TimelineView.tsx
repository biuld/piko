// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
// ============================================================================

import type { ScrollBoxRenderable } from "@opentui/core";
import { createEffect, createSignal } from "solid-js";
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
  let scrollboxEl: ScrollBoxRenderable | undefined;
  // Track previous stickyBottom to detect false → true edge
  const [prevSticky, setPrevSticky] = createSignal(props.stickyBottom);

  // When stickyBottom transitions from false → true, force scroll to bottom.
  createEffect(() => {
    const current = props.stickyBottom;
    const prev = prevSticky();
    if (current && !prev && scrollboxEl) {
      queueMicrotask(() => {
        scrollboxEl?.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
      });
    }
    setPrevSticky(current);
  });

  return (
    <box flexDirection="column" flexGrow={1} overflow="hidden">
      <scrollbox
        ref={(el: ScrollBoxRenderable) => { scrollboxEl = el; }}
        flexGrow={1}
        flexShrink={1}
        height="100%"
        stickyScroll={props.stickyBottom}
        stickyStart="bottom"
      >
        {props.items.map((item, i) => (
          <>
            {i > 0 && <TimelineSeparator />}
            <TimelineItemView
              item={item}
              layout={props.layout}
              isExpanded={props.expandedItemIds.has(item.id)}
              isCollapsed={props.collapsedToolCallIds.has(item.toolCallId ?? "")}
            />
          </>
        ))}
      </scrollbox>

      {props.pendingNewItems > 0 && (
        <LatestIndicator count={props.pendingNewItems} />
      )}
    </box>
  );
}
