// ============================================================================
// AssistantMessageView — render assistant messages with markdown, thinking,
// and error states. Pi-aligned rendering.
//
// Pi pattern:
//   - Text blocks rendered as Markdown with fg=text.primary
//   - Thinking blocks rendered in italic thinkingText color (hideable)
//   - Error/aborted messages shown in error color
//   - Spacer between visible content blocks
// ============================================================================

import { TextAttributes } from "@opentui/core";
import type { TimelineItem } from "../../../timeline/types.js";
import { MarkdownContent } from "./MarkdownContent.js";
import { useTheme } from "../theme-context.js";

export interface AssistantMessageViewProps {
  item: TimelineItem;
}

export function AssistantMessageView(props: AssistantMessageViewProps) {
  const theme = useTheme();
  const { item } = props;

  const hasText = item.text && item.text.trim().length > 0;
  const hasThinking = item.thinkingText && item.thinkingText.trim().length > 0;
  const hideThinking = item.hideThinking ?? false;
  const isError = item.isError ?? false;
  const errorMessage = item.errorMessage;
  const isStreaming = item.isStreaming ?? false;

  // Nothing to show
  if (!hasText && !hasThinking && !isError) {
    if (isStreaming) {
      return (
        <box paddingLeft={1} paddingRight={1}>
          <text fg={theme.color("text.muted")}>...</text>
        </box>
      );
    }
    return null;
  }

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1}>
      <box height={1} />
      {/* Thinking block — rendered before text, in italic thinkingText color */}
      {hasThinking && !hideThinking && (
        <box paddingTop={hasText ? 1 : 0} paddingBottom={hasText ? 1 : 0}>
          <text
            fg={theme.color("thinking.text")}
            attributes={TextAttributes.ITALIC}
          >
            {item.thinkingText!}
          </text>
        </box>
      )}

      {/* Hidden thinking label */}
      {hasThinking && hideThinking && (
        <box paddingTop={1}>
          <text
            fg={theme.color("thinking.hiddenLabel")}
            attributes={TextAttributes.ITALIC}
          >
            Thinking...
          </text>
        </box>
      )}

      {/* Main text content — rendered as Markdown */}
      {hasText && (
        <MarkdownContent
          content={item.text!}
          fg={theme.color("text.primary")}
          streaming={isStreaming}
          conceal={true}
        />
      )}

      {/* Error / aborted message */}
      {isError && errorMessage && (
        <box paddingTop={hasText || hasThinking ? 1 : 0}>
          <text fg={theme.color("text.error")}>
            {errorMessage}
          </text>
        </box>
      )}
    </box>
  );
}
