// ============================================================================
// ChatView — scrollable message list with markdown rendering
// ============================================================================

import type { LayoutMode, TuiMessageViewModel } from "../../state/state.js";

export interface ChatViewProps {
  transcript: TuiMessageViewModel[];
  mode: LayoutMode;
  isStreaming: boolean;
}

export function ChatView(props: ChatViewProps) {
  const { transcript, isStreaming } = props;

  return (
    <scrollbox flexGrow={1} flexShrink={1} height="100%">
      {transcript.map((msg) => {
        switch (msg.role) {
          case "user":
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                <text fg="#8abeb7">
                  <strong>You</strong>
                </text>
                <box paddingLeft={2}>
                  <text>{msg.text}</text>
                </box>
              </box>
            );

          case "assistant":
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                {msg.text ? (
                  <text>{msg.text}</text>
                ) : (
                  <text fg="#808080">...</text>
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
                ? "#f0c674"
                : tb.status === "success"
                  ? "#b5bd68"
                  : tb.status === "error"
                    ? "#cc6666"
                    : "#808080";

            // Format args for display
            const argsStr =
              tb.args && typeof tb.args === "object"
                ? JSON.stringify(tb.args).slice(0, 200)
                : "";

            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
                <box flexDirection="row">
                  <text fg={statusColor}>{statusIcon} </text>
                  <text fg="#d4d4d4">
                    <strong>[tool] {tb.name}</strong>
                  </text>
                  {argsStr && (
                    <text fg="#808080"> {argsStr}</text>
                  )}
                </box>
                {tb.result !== undefined && (
                  <box paddingLeft={4}>
                    <text fg="#808080">
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
                <text fg="#9575cd">📋 Branch summary: {msg.text}</text>
              </box>
            );

          case "compactionSummary":
            return (
              <box paddingLeft={1} paddingRight={1} paddingTop={1}>
                <text fg="#9575cd">📦 Compaction: {msg.text}</text>
              </box>
            );

          default:
            return (
              <box paddingLeft={1} paddingRight={1}>
                <text fg="#808080">{msg.text}</text>
              </box>
            );
        }
      })}
    </scrollbox>
  );
}
