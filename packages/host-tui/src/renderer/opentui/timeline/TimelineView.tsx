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
  /** Counter that increments on page-up, page-down, or jump-latest requests */
  scrollCommand: { dir: "pageUp" | "pageDown" | "jumpLatest" } | null;
}

export function TimelineView(props: TimelineViewProps) {
  let scrollboxEl: ScrollBoxRenderable | undefined;
  let prevSticky = props.stickyBottom;

  // Watch scrollCommand: when it changes, dispatch to scrollbox
  createEffect(() => {
    const cmd = props.scrollCommand;
    if (!cmd || !scrollboxEl) return;

    if (cmd.dir === "jumpLatest") {
      scrollboxEl.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
    } else if (cmd.dir === "pageUp") {
      scrollboxEl.scrollBy({ x: 0, y: -scrollboxEl.scrollHeight * 0.5 });
    } else if (cmd.dir === "pageDown") {
      scrollboxEl.scrollBy({ x: 0, y: scrollboxEl.scrollHeight * 0.5 });
    }
  });

  // Edge-detect stickyBottom false → true: force scroll to bottom
  createEffect(() => {
    const current = props.stickyBottom;
    const prev = prevSticky;
    prevSticky = current;
    if (current && !prev && scrollboxEl) {
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
