// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
//
// Uses projection.orderedIds + projection.itemsById for deterministic,
// stable-ID-keyed rendering. SolidJS For with keyed items preserves
// component identity across updates (no remounting).
// ============================================================================

import type { ScrollBoxRenderable } from "@opentui/core";
import type { PikoHost } from "piko-host-runtime";
import { createEffect, createMemo, For, onCleanup, Show } from "solid-js";
import type { TimelineProjection } from "../../../timeline/projection.js";
import type { TimelineLayout } from "../../../timeline/types.js";
import { LatestIndicator } from "./LatestIndicator.js";
import { TimelineItemView } from "./TimelineItemView.js";
import { WelcomeBanner } from "./WelcomeBanner.js";

export interface TimelineViewProps {
  projection: TimelineProjection;
  layout: TimelineLayout;
  pendingNewItems: number;
  expandedItemIds: Set<string>;
  collapsedToolCallIds: Set<string>;
  stickyBottom: boolean;
  streamRunning: boolean;
  scrollCommand: { dir: "pageUp" | "pageDown" | "jumpLatest"; seq: number } | null;
  onScrollStateChange?: (atBottom: boolean) => void;
  onScrollCommandDone?: () => void;
  host: PikoHost;
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
    const contentGrew = lastScrollHeight >= 0 && scrollHeight > lastScrollHeight;
    const didNotScrollUp = lastScrollTop < 0 || scrollTop >= lastScrollTop;
    const transientStreamingGrowth =
      props.streamRunning &&
      props.stickyBottom &&
      !atBottom &&
      lastAtBottom !== false &&
      contentGrew &&
      didNotScrollUp;

    if (
      atBottom !== lastAtBottom ||
      scrollTop !== lastScrollTop ||
      scrollHeight !== lastScrollHeight ||
      viewportHeight !== lastViewportHeight
    ) {
      lastScrollTop = scrollTop;
      lastScrollHeight = scrollHeight;
      lastViewportHeight = viewportHeight;
      if (transientStreamingGrowth) {
        queueMicrotask(() => {
          scrollboxEl?.scrollTo({ x: 0, y: Number.MAX_SAFE_INTEGER });
        });
        return;
      }
      // Only dispatch when atBottom actually transitions
      if (atBottom !== lastAtBottom) {
        lastAtBottom = atBottom;
        props.onScrollStateChange?.(atBottom);
      }
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

  const orderedIds = () => props.projection.orderedIds;
  const itemsById = () => props.projection.itemsById;

  return (
    <box flexDirection="column" flexGrow={1} overflow="hidden">
      <Show
        when={orderedIds().length > 0}
        fallback={<WelcomeBanner host={props.host} width={props.layout.width} />}
      >
        <scrollbox
          ref={handleRef}
          flexGrow={1}
          flexShrink={1}
          height="100%"
          stickyScroll={props.stickyBottom}
          stickyStart={props.stickyBottom ? "bottom" : "top"}
        >
          <For each={orderedIds()}>
            {(id) => {
              // Reactive lookup: item re-computes when itemsById changes
              const item = createMemo(() => itemsById()[id]);
              return (
                <Show when={item()}>
                  {(it) => (
                    <TimelineItemView
                      item={it()}
                      layout={props.layout}
                      isExpanded={props.expandedItemIds.has(it().id)}
                      isCollapsed={props.collapsedToolCallIds.has(it().toolCallId ?? "")}
                    />
                  )}
                </Show>
              );
            }}
          </For>
        </scrollbox>
      </Show>

      {props.pendingNewItems > 0 && <LatestIndicator count={props.pendingNewItems} />}
    </box>
  );
}
