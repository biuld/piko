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
  pendingNewItems: number;
  expandedItemIds: Set<string>;
  collapsedToolCallIds: Set<string>;
  /** Whether to auto-stick to bottom (true when user hasn't manually scrolled away) */
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

  return (
    <box flexDirection="column" flexGrow={1} overflow="hidden">
      <scrollbox
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
