// ============================================================================
// ChatView — scrollable message list with semantic theme tokens
// ============================================================================

import type { LayoutMode, TuiMessageViewModel } from "../../state/state.js";
import { useTheme } from "./theme-context.js";
import { ToolBlock } from "./tools/ToolBlock.js";

export interface ChatViewProps {
  transcript: TuiMessageViewModel[];
  mode: LayoutMode;
  isStreaming: boolean;
}

export function ChatView(props: ChatViewProps) {
  const theme = useTheme();
  const { transcript } = props;

  return (
    <scrollbox flexGrow={1} flexShrink={1} height="100%">
      {transcript.map((msg) => {
        switch (msg.role) {
          case "user":
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                <text fg={theme.color("text.accent")}>
                  <strong>You</strong>
                </text>
                <box paddingLeft={2}>
                  <text fg={theme.color("text.primary")}>{msg.text}</text>
                </box>
              </box>
            );

          case "assistant":
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                {msg.text ? (
                  <text fg={theme.color("text.primary")}>{msg.text}</text>
                ) : (
                  <text fg={theme.color("text.muted")}>...</text>
                )}
              </box>
            );

          case "tool": {
            const tb = msg.toolBlock;
            if (!tb) return null;
            return <ToolBlock block={tb} />;
          }

          case "branchSummary":
            return (
              <box paddingLeft={1} paddingRight={1} paddingTop={1}>
                <text fg={theme.color("thinking.text")}>📋 Branch summary: {msg.text}</text>
              </box>
            );

          case "compactionSummary":
            return (
              <box paddingLeft={1} paddingRight={1} paddingTop={1}>
                <text fg={theme.color("thinking.text")}>📦 Compaction: {msg.text}</text>
              </box>
            );

          default:
            return (
              <box paddingLeft={1} paddingRight={1}>
                <text fg={theme.color("text.muted")}>{msg.text}</text>
              </box>
            );
        }
      })}
    </scrollbox>
  );
}
