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
  scrollCommand: { dir: "pageUp" | "pageDown" | "jumpLatest"; seq: number } | null;
  onScrollCommandDone?: (dir: string, atBottom: boolean) => void;
}

export function TimelineView(props: TimelineViewProps) {
  let scrollboxEl: ScrollBoxRenderable | undefined;
  let prevSticky = props.stickyBottom;
  let lastConsumedSeq = -1;

  // Watch scrollCommand: when seq changes, execute the command
  createEffect(() => {
    const cmd = props.scrollCommand;
    if (!cmd || !scrollboxEl || cmd.seq === lastConsumedSeq) return;
    lastConsumedSeq = cmd.seq;

    const pageHeight = props.layout.height * 0.7; // ~70% of timeline area is one "page"

    if (cmd.dir === "jumpLatest") {
      scrollboxEl.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
    } else if (cmd.dir === "pageUp") {
      scrollboxEl.scrollBy({ x: 0, y: -pageHeight });
    } else if (cmd.dir === "pageDown") {
      scrollboxEl.scrollBy({ x: 0, y: pageHeight });
    }

    // After scroll, check if we're at bottom
    queueMicrotask(() => {
      if (!scrollboxEl) return;
      const atBottom =
        scrollboxEl.scrollTop + scrollboxEl.viewport.height >=
        scrollboxEl.scrollHeight - 2;
      props.onScrollCommandDone?.(cmd.dir, atBottom);
    });
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
