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
  let pollTimer: ReturnType<typeof setInterval> | undefined;

  // Track previous state to detect changes in any dimension
  let lastScrollTop = -1;
  let lastScrollHeight = -1;
  let lastViewportHeight = -1;
  let lastAtBottom: boolean | undefined;

  // ------ Poll + report scroll state ------
  const reportScrollState = () => {
    if (!scrollboxEl) return;
    const scrollTop = scrollboxEl.scrollTop;
    const scrollHeight = scrollboxEl.scrollHeight;
    const viewportHeight = scrollboxEl.viewport.height;
    const atBottom = scrollTop + viewportHeight >= scrollHeight - 2;

    if (
      atBottom !== lastAtBottom ||
      scrollTop !== lastScrollTop ||
      scrollHeight !== lastScrollHeight ||
      viewportHeight !== lastViewportHeight
    ) {
      lastScrollTop = scrollTop;
      lastScrollHeight = scrollHeight;
      lastViewportHeight = viewportHeight;
      lastAtBottom = atBottom;
      props.onScrollStateChange?.(atBottom);
    }
  };

  const startPolling = () => {
    if (pollTimer) return;
    pollTimer = setInterval(reportScrollState, 200);
  };

  const stopPolling = () => {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  };

  // Start polling when scrollbox ref attaches, stop on cleanup
  const handleRef = (el: ScrollBoxRenderable) => {
    scrollboxEl = el;
    startPolling();
  };
  onCleanup(stopPolling);

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

    queueMicrotask(() => {
      reportScrollState();
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
        reportScrollState();
      });
    }
  });

  return (
    <box flexDirection="column" flexGrow={1} overflow="hidden">
      <scrollbox
        ref={handleRef}
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
