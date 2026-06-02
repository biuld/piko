// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
// ============================================================================

import type { TimelineItem, TimelineLayout } from "../../../timeline/types.js";
import { TimelineItemView } from "./TimelineItemView.js";
import { TimelineSeparator } from "./TimelineSeparator.js";
import { LatestIndicator } from "./LatestIndicator.js";

export interface TimelineViewProps {
  items: TimelineItem[];
  layout: TimelineLayout;
  isStreaming: boolean;
  pendingNewItems: number;
  expandedItemIds: Set<string>;
  collapsedToolCallIds: Set<string>;
}

export function TimelineView(props: TimelineViewProps) {
  const {
    items,
    layout,
    isStreaming,
    pendingNewItems,
    expandedItemIds,
    collapsedToolCallIds,
  } = props;

  return (
    <scrollbox flexGrow={1} flexShrink={1} height="100%" stickyScroll={true} stickyStart="bottom">
      {/* Timeline items with separators */}
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

      {/* Latest indicator at bottom — visible when new items are pending
          (user scrolled away from bottom while streaming) */}
      {pendingNewItems > 0 && (
        <LatestIndicator count={pendingNewItems} />
      )}
    </scrollbox>
  );
}
