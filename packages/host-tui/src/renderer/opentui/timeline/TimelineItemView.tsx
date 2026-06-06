// ============================================================================
// TimelineItemView — dispatches to type-specific renderers
// ============================================================================

import type { TimelineItem, TimelineLayout } from "../../../timeline/types.js";
import { UserMessageView } from "./UserMessageView.js";
import { AssistantMessageView } from "./AssistantMessageView.js";
import { ToolTimelineItem } from "./ToolTimelineItem.js";
import { SummaryTimelineItem } from "./SummaryTimelineItem.js";
import { useTheme } from "../theme-context.js";

export interface TimelineItemViewProps {
  item: TimelineItem;
  layout: TimelineLayout;
  isExpanded: boolean;
  isCollapsed: boolean;
}

export function TimelineItemView(props: TimelineItemViewProps) {
  const { item, layout, isExpanded, isCollapsed } = props;
  const theme = useTheme();

  switch (item.kind) {
    case "user-message":
      return <UserMessageView item={item} />;

    case "assistant-message":
    case "assistant-stream":
      return <AssistantMessageView item={item} />;

    case "tool-call":
    case "tool-result":
      return (
        <ToolTimelineItem
          item={item}
          isExpanded={isExpanded}
          isCollapsed={isCollapsed}
        />
      );

    case "branch-summary":
    case "compaction-summary":
      return <SummaryTimelineItem item={item} />;

    case "approval":
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.warning")}>
            ⚠️ Approve `{item.toolName ?? "unknown"}`?
          </text>
        </box>
      );

    case "system-note":
    case "notification-ref":
    default:
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.muted")}>{item.text}</text>
        </box>
      );
  }
}
