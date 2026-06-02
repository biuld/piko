// ============================================================================
// AssistantMessageView — render assistant messages and streaming content
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";

export interface AssistantMessageViewProps {
  item: TimelineItem;
}

export function AssistantMessageView(props: AssistantMessageViewProps) {
  const theme = useTheme();
  const { item } = props;

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1}>
      {item.text ? (
        <text fg={theme.color("text.primary")}>{item.text}</text>
      ) : item.isStreaming ? (
        <text fg={theme.color("text.muted")}>...</text>
      ) : null}
    </box>
  );
}
