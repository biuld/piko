// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
// ============================================================================

import type { ScrollBoxRenderable } from "@opentui/core";
import { createEffect, onCleanup } from "solid-js";
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
  onScrollStateChange?: (atBottom: boolean) => void;
  onScrollCommandDone?: () => void;
}

export function TimelineView(props: TimelineViewProps) {
  let scrollboxEl: ScrollBoxRenderable | undefined;
  let prevSticky = props.stickyBottom;
  let lastConsumedSeq = -1;
  let lastScrollTop = 0;
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  // ------ Poll scroll position for real scroll detection ------
  const checkScrollState = () => {
    if (!scrollboxEl) return;
    const scrollTop = scrollboxEl.scrollTop;
    if (scrollTop === lastScrollTop) return;
    lastScrollTop = scrollTop;

    const atBottom =
      scrollTop + scrollboxEl.viewport.height >= scrollboxEl.scrollHeight - 2;
    props.onScrollStateChange?.(atBottom);
  };

  // Start polling when scrollbox ref is attached. 200ms interval is
  // responsive enough for scroll detection without being expensive.
  createEffect(() => {
    if (scrollboxEl) {
      pollTimer = setInterval(checkScrollState, 200);
    }
    onCleanup(() => {
      if (pollTimer) clearInterval(pollTimer);
    });
  });

  // ------ React to scrollCommand ------
  createEffect(() => {
    const cmd = props.scrollCommand;
    if (!cmd || !scrollboxEl || cmd.seq === lastConsumedSeq) return;
    lastConsumedSeq = cmd.seq;

    const pageHeight = props.layout.height * 0.7;

    if (cmd.dir === "jumpLatest") {
      scrollboxEl.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
    } else if (cmd.dir === "pageUp") {
      scrollboxEl.scrollBy({ x: 0, y: -pageHeight });
    } else if (cmd.dir === "pageDown") {
      scrollboxEl.scrollBy({ x: 0, y: pageHeight });
    }

    // Check state after scroll settles
    queueMicrotask(() => {
      checkScrollState();
      props.onScrollCommandDone?.();
    });
  });

  // ------ stickyBottom false → true edge: force scroll to bottom ------
  createEffect(() => {
    const current = props.stickyBottom;
    const prev = prevSticky;
    prevSticky = current;
    if (current && !prev && scrollboxEl) {
      queueMicrotask(() => {
        scrollboxEl?.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
        checkScrollState();
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
