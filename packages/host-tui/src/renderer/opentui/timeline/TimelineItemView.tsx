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
  const theme = useTheme();

  switch (props.item.kind) {
    case "user-message":
      return <UserMessageView item={props.item} />;

    case "assistant-message":
    case "assistant-stream":
      return <AssistantMessageView item={props.item} />;

    case "tool-call":
    case "tool-result":
      return (
        <ToolTimelineItem
          item={props.item}
          isExpanded={props.isExpanded}
          isCollapsed={props.isCollapsed}
        />
      );

    case "branch-summary":
    case "compaction-summary":
      return <SummaryTimelineItem item={props.item} isExpanded={props.isExpanded} />;

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
              [approval] Approve `{props.item.toolName ?? "unknown"}`?
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
            {props.item.customType ? (
              <text
                fg={theme.color("text.customLabel")}
                attributes={TextAttributes.BOLD}
              >
                [{props.item.customType}]
              </text>
            ) : null}
            {props.item.text ? (
              <text fg={theme.color("text.primary")}>{props.item.text}</text>
            ) : null}
          </box>
        </box>
      );

    case "notification-ref":
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.warning")}>{props.item.text}</text>
        </box>
      );

    default:
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.muted")}>{props.item.text}</text>
        </box>
      );
  }
}
