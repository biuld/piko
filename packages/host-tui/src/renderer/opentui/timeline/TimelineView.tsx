// ============================================================================
// TimelineView — main timeline entry point, replaces ChatView
// ============================================================================

import type { TimelineItem, TimelineLayout } from "../../../timeline/types.js";
import { TimelineItemView } from "./TimelineItemView.js";
import { LatestIndicator } from "./LatestIndicator.js";
import { selectPendingCount } from "../../../timeline/timeline-selectors.js";

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
    <scrollbox flexGrow={1} flexShrink={1} height="100%">
      {pendingNewItems > 0 && (
        <LatestIndicator count={pendingNewItems} />
      )}

      {items.map((item, i) => (
        <TimelineItemView
          item={item}
          layout={layout}
          isExpanded={expandedItemIds.has(item.id)}
          isCollapsed={collapsedToolCallIds.has(item.toolCallId ?? "")}
        />
      ))}
    </scrollbox>
  );
}
