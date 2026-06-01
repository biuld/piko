// ============================================================================
// ChatView — scrollable message list with separators and theme tokens
// ============================================================================

import type { LayoutMode, TuiMessageViewModel } from "../../state/state.js";
import { useTheme } from "./theme-context.js";
import { ToolBlock } from "./tools/ToolBlock.js";

export interface ChatViewProps {
  transcript: TuiMessageViewModel[];
  mode: LayoutMode;
  isStreaming: boolean;
}

/** Subtle horizontal separator between messages */
function MessageSeparator() {
  const theme = useTheme();
  return (
    <box height={1} paddingLeft={1} paddingRight={1}>
      <text fg={theme.color("border.muted")}>───</text>
    </box>
  );
}

export function ChatView(props: ChatViewProps) {
  const theme = useTheme();
  const { transcript } = props;

  return (
    <scrollbox flexGrow={1} flexShrink={1} height="100%">
      {transcript.map((msg, i) => (
        <>
          {/* Separator between messages (not before first) */}
          {i > 0 && <MessageSeparator />}

          {msg.role === "user" ? (
            <box flexDirection="column" paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("text.accent")}>
                <strong>You</strong>
              </text>
              <box paddingLeft={2}>
                <text fg={theme.color("text.primary")}>{msg.text}</text>
              </box>
            </box>
          ) : msg.role === "assistant" ? (
            <box flexDirection="column" paddingLeft={1} paddingRight={1}>
              {msg.text ? (
                <text fg={theme.color("text.primary")}>{msg.text}</text>
              ) : (
                <text fg={theme.color("text.muted")}>...</text>
              )}
            </box>
          ) : msg.role === "tool" && msg.toolBlock ? (
            <ToolBlock block={msg.toolBlock} />
          ) : msg.role === "branchSummary" ? (
            <box paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("thinking.text")}>📋 {msg.text}</text>
            </box>
          ) : msg.role === "compactionSummary" ? (
            <box paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("thinking.text")}>📦 {msg.text}</text>
            </box>
          ) : (
            <box paddingLeft={1} paddingRight={1}>
              <text fg={theme.color("text.muted")}>{msg.text}</text>
            </box>
          )}
        </>
      ))}
    </scrollbox>
  );
}
