import { TextAttributes } from "@opentui/core";
import { Match, Switch } from "solid-js";
import type { TimelineItem, TimelineLayout } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";
import { AssistantMessageView } from "./AssistantMessageView.js";
import { SummaryTimelineItem } from "./SummaryTimelineItem.js";
import { ToolTimelineItem } from "./ToolTimelineItem.js";
import { UserMessageView } from "./UserMessageView.js";

export interface TimelineItemViewProps {
  item: TimelineItem;
  layout: TimelineLayout;
  isExpanded: boolean;
  isCollapsed: boolean;
}

export function TimelineItemView(props: TimelineItemViewProps) {
  const theme = useTheme();

  return (
    <Switch
      fallback={
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.muted")}>{props.item.text}</text>
        </box>
      }
    >
      <Match when={props.item.kind === "user-message"}>
        <UserMessageView item={props.item} />
      </Match>
      <Match
        when={props.item.kind === "assistant-message" || props.item.kind === "assistant-stream"}
      >
        <AssistantMessageView item={props.item} />
      </Match>
      <Match when={props.item.kind === "tool-call" || props.item.kind === "tool-result"}>
        <ToolTimelineItem
          item={props.item}
          isExpanded={props.isExpanded}
          isCollapsed={props.isCollapsed}
        />
      </Match>
      <Match
        when={props.item.kind === "branch-summary" || props.item.kind === "compaction-summary"}
      >
        <SummaryTimelineItem item={props.item} isExpanded={props.isExpanded} />
      </Match>
      <Match when={props.item.kind === "approval"}>
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
      </Match>
      <Match when={props.item.kind === "system-note"}>
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
              <text fg={theme.color("text.customLabel")} attributes={TextAttributes.BOLD}>
                [{props.item.customType}]
              </text>
            ) : null}
            {props.item.text ? (
              <text fg={theme.color("text.primary")}>{props.item.text}</text>
            ) : null}
          </box>
        </box>
      </Match>
      <Match when={props.item.kind === "notification-ref"}>
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.warning")}>{props.item.text}</text>
        </box>
      </Match>
    </Switch>
  );
}
