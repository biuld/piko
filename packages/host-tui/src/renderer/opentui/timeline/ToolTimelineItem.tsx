// ============================================================================
// ToolTimelineItem — render tool calls/results in the timeline
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";

export interface ToolTimelineItemProps {
  item: TimelineItem;
  isExpanded: boolean;
  isCollapsed: boolean;
}

function statusColor(theme: ReturnType<typeof useTheme>, status?: string): string {
  switch (status) {
    case "running":
      return String(theme.color("text.accent"));
    case "success":
      return String(theme.color("ok"));
    case "error":
      return String(theme.color("text.error"));
    default:
      return String(theme.color("text.muted"));
  }
}

function statusIcon(status?: string): string {
  switch (status) {
    case "running":
      return "○";
    case "success":
      return "✓";
    case "error":
      return "✗";
    case "pending":
      return "○";
    default:
      return "•";
  }
}

export function ToolTimelineItem(props: ToolTimelineItemProps) {
  const theme = useTheme();
  const { item, isExpanded, isCollapsed } = props;

  const showSummary = !isCollapsed;
  const showDetails = isExpanded;

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      {/* Status line */}
      <box flexDirection="row" height={1}>
        <text fg={statusColor(theme, item.toolStatus)}>
          {statusIcon(item.toolStatus)} {item.toolName ?? "tool"}
        </text>
        {item.toolResult != null && (
          <text fg={theme.color("text.dim")}>
            {" "}{isExpanded ? "▲" : "▶"}
          </text>
        )}
      </box>

      {/* Expanded details */}
      {showDetails && item.toolResult != null && (
        <box paddingLeft={4} paddingTop={1} flexDirection="column">
          <text fg={theme.color("tool.output")}>
            {typeof item.toolResult === "string"
              ? item.toolResult
              : JSON.stringify(item.toolResult, null, 2)}
          </text>
        </box>
      )}
    </box>
  );
}
