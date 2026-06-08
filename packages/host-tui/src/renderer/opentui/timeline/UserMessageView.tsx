// ============================================================================
// UserMessageView — render user messages, pi-aligned
//
// Pi pattern:
//   - Single Box with userMessageBg background, paddingX=1 inside bg
//   - Content indented 1 cell inside the colored background
//   - Full Markdown rendering
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";
import { MarkdownContent } from "./MarkdownContent.js";

export interface UserMessageViewProps {
  item: TimelineItem;
}

export function UserMessageView(props: UserMessageViewProps) {
  const theme = useTheme();
  const { item } = props;

  if (!item.text) return null;

  return (
    <box flexDirection="column">
      <box height={1} />
      <box
        backgroundColor={theme.color("surface.userMessage")}
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        flexDirection="column"
      >
        <MarkdownContent
          content={item.text}
          fg={theme.color("text.primary")}
          bg={theme.color("surface.userMessage")}
          conceal={true}
        />
      </box>
    </box>
  );
}
