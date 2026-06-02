// ============================================================================
// UserMessageView — render user messages in the timeline
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";

export interface UserMessageViewProps {
  item: TimelineItem;
}

export function UserMessageView(props: UserMessageViewProps) {
  const theme = useTheme();
  const { item } = props;

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1}>
      <text fg={theme.color("text.accent")}>
        <strong>You</strong>
      </text>
      <box paddingLeft={2}>
        <text fg={theme.color("text.primary")}>{item.text}</text>
      </box>
    </box>
  );
}
