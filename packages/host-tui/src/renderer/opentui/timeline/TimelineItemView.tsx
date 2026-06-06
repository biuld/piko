// ============================================================================
// TimelineItemView — dispatches to type-specific renderers
// ============================================================================

import { TextAttributes } from "@opentui/core";
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
  const { item, isExpanded, isCollapsed } = props;
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
      return <SummaryTimelineItem item={item} isExpanded={isExpanded} />;

    case "approval":
      return (
        <box flexDirection="column">
          <box height={1} />
          <box
            backgroundColor={theme.color("surface.toolPending")}
            paddingLeft={1}
            paddingRight={1}
            paddingTop={1}
            paddingBottom={1}
          >
            <text fg={theme.color("text.warning")}>
              [approval] Approve `{item.toolName ?? "unknown"}`?
            </text>
          </box>
        </box>
      );

    case "system-note":
      // Pi: custom messages use [customType] badge + customMessageBg background
      return (
        <box flexDirection="column">
          <box height={1} />
          <box
            backgroundColor={theme.color("surface.customMessage")}
            paddingLeft={1}
            paddingRight={1}
            paddingTop={1}
            paddingBottom={1}
          >
            {item.customType ? (
              <text
                fg={theme.color("text.customLabel")}
                attributes={TextAttributes.BOLD}
              >
                [{item.customType}]
              </text>
            ) : null}
            {item.text ? (
              <text fg={theme.color("text.primary")}>{item.text}</text>
            ) : null}
          </box>
        </box>
      );

    case "notification-ref":
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.warning")}>{item.text}</text>
        </box>
      );

    default:
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.muted")}>{item.text}</text>
        </box>
      );
  }
}
