// ============================================================================
// ChatView — scrollable message list with semantic theme tokens
// ============================================================================

import type { LayoutMode, TuiMessageViewModel } from "../../state/state.js";
import { useTheme } from "./theme-context.js";

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
            const statusIcon =
              tb.status === "running"
                ? "⏳"
                : tb.status === "success"
                  ? "✓"
                  : tb.status === "error"
                    ? "✗"
                    : "○";
            const statusColor =
              tb.status === "running"
                ? theme.color("text.warning")
                : tb.status === "success"
                  ? theme.color("text.success")
                  : tb.status === "error"
                    ? theme.color("text.error")
                    : theme.color("text.muted");

            const argsStr =
              tb.args && typeof tb.args === "object"
                ? JSON.stringify(tb.args).slice(0, 200)
                : "";

            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                <box flexDirection="row">
                  <text fg={statusColor}>{statusIcon} </text>
                  <text fg={theme.color("tool.title")}>
                    <strong>[tool] {tb.name}</strong>
                  </text>
                  {argsStr && (
                    <text fg={theme.color("tool.args")}> {argsStr}</text>
                  )}
                </box>
                {tb.result !== undefined && (
                  <box paddingLeft={4}>
                    <text fg={theme.color("tool.output")}>
                      {typeof tb.result === "string"
                        ? tb.result.slice(0, 500)
                        : JSON.stringify(tb.result).slice(0, 500)}
                    </text>
                  </box>
                )}
              </box>
            );
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
