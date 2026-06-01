// ============================================================================
// ChatView — scrollable message list
// ============================================================================

import type { LayoutMode, TuiMessageViewModel } from "../../state/state.js";

export interface ChatViewProps {
  transcript: TuiMessageViewModel[];
  mode: LayoutMode;
  isStreaming: boolean;
}

export function ChatView(props: ChatViewProps) {
  const { transcript, mode, isStreaming } = props;

  return (
    <scrollbox flexGrow={1} flexShrink={1} height="100%">
      {transcript.map((msg) => {
        switch (msg.role) {
          case "user":
            return (
              <box flexDirection="row" paddingLeft={1} paddingRight={1}>
                <text fg="#8abeb7">You: </text>
                <text>{msg.text}</text>
              </box>
            );

          case "assistant":
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1}>
                {msg.isStreaming ? (
                  <text>{msg.text}</text>
                ) : (
                  <text>{msg.text || "..."}</text>
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
            return (
              <box flexDirection="column" paddingLeft={1} paddingRight={1}>
                <text fg={statusColor}>
                  {statusIcon} [tool] {tb.name}
                </text>
                {tb.result !== undefined && !msg.isStreaming && (
                  <text fg="#808080">
                    {typeof tb.result === "string"
                      ? tb.result.slice(0, 200)
                      : JSON.stringify(tb.result).slice(0, 200)}
                  </text>
                )}
              </box>
            );
          }

          case "branchSummary":
            return (
              <box flexDirection="row" paddingLeft={1} paddingRight={1}>
                <text fg="#9575cd">📋 Branch summary: </text>
                <text fg="#808080">{msg.text}</text>
              </box>
            );

          case "compactionSummary":
            return (
              <box flexDirection="row" paddingLeft={1} paddingRight={1}>
                <text fg="#9575cd">📦 Compaction: </text>
                <text fg="#808080">{msg.text}</text>
              </box>
            );

          default:
            return <text fg="#808080">{msg.text}</text>;
        }
      })}
    </scrollbox>
  );
}
