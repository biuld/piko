// ============================================================================
// ToolTimelineItem — render tool calls/results, pi-aligned
//
// Pi pattern:
//   compact:  toolTitle classification args   (Ctrl+O to expand)
//   expanded: full result output with line limit
//   No status icons — status conveyed by color alone
// ============================================================================

import type { TimelineItem } from "../../../timeline/types.js";
import { useTheme } from "../theme-context.js";

export interface ToolTimelineItemProps {
  item: TimelineItem;
  isExpanded: boolean;
  isCollapsed: boolean;
}

// ============================================================================
// Tool arg formatters — produce pi-style compact call line
// ============================================================================

function formatToolArgs(item: TimelineItem): string {
  const toolName = item.toolName ?? "tool";
  const args = (item.toolArgs ?? {}) as Record<string, unknown>;

  switch (toolName) {
    case "read": {
      const path = str(args.file_path ?? args.path);
      const offset = num(args.offset);
      const limit = num(args.limit);
      if (offset !== undefined && limit !== undefined) {
        return `read ${path} (L${offset + 1}-${offset + limit})`;
      }
      return `read ${path}`;
    }
    case "write": {
      return `write ${str(args.file_path ?? args.path)}`;
    }
    case "edit": {
      return `edit ${str(args.file_path ?? args.path)}`;
    }
    case "bash": {
      const cmd = str(args.command);
      if (cmd) return `bash ${cmd}`;
      return "bash";
    }
    case "grep": {
      const pattern = str(args.pattern);
      const dir = str(args.path ?? args.directory);
      if (pattern && dir) return `grep ${pattern} in ${dir}`;
      if (pattern) return `grep ${pattern}`;
      return "grep";
    }
    case "find": {
      return `find ${str(args.pattern ?? args.glob ?? "")}`;
    }
    case "ls": {
      return `ls ${str(args.path ?? args.directory ?? "")}`;
    }
    default:
      return toolName;
  }
}

// ============================================================================
// Color helpers
// ============================================================================

function statusFg(
  theme: ReturnType<typeof useTheme>,
  status?: string,
  isError?: boolean,
): string {
  if (isError) return String(theme.color("text.error"));
  switch (status) {
    case "running":
      return String(theme.color("text.accent"));
    case "error":
      return String(theme.color("text.error"));
    default:
      return String(theme.color("text.muted"));
  }
}

function resultFg(theme: ReturnType<typeof useTheme>): string {
  return String(theme.color("text.muted"));
}

// ============================================================================
// Helpers
// ============================================================================

function str(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === undefined || value === null) return "";
  return String(value);
}

function num(value: unknown): number | undefined {
  if (typeof value === "number") return value;
  return undefined;
}

// ============================================================================
// Component
// ============================================================================

export function ToolTimelineItem(props: ToolTimelineItemProps) {
  const theme = useTheme();
  const { item, isExpanded, isCollapsed } = props;
  const isError =
    item.toolStatus === "error" || (item.toolResult !== null && item.toolResult !== undefined && typeof item.toolResult === "object" && (item.toolResult as any)?.isError === true);

  // ---- Compact call line (always shown) ----
  const callText = formatToolArgs(item);
  const expandHint = item.toolResult != null ? " (Ctrl+O to expand)" : "";

  // ---- Result (only when expanded) ----
  const showResult = isExpanded && item.toolResult != null;
  let resultText = "";
  if (showResult) {
    resultText =
      typeof item.toolResult === "string"
        ? item.toolResult
        : JSON.stringify(item.toolResult, null, 2);
  }

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      {/* Call line */}
      <box flexDirection="row" height={1}>
        <text fg={statusFg(theme, item.toolStatus, isError)}>
          {callText}
        </text>
        {item.toolResult != null && !isExpanded && (
          <text fg={theme.color("text.dim")}>
            {expandHint}
          </text>
        )}
        {isExpanded && (
          <text fg={theme.color("text.dim")}> ▲</text>
        )}
      </box>

      {/* Expanded result */}
      {showResult && (
        <box paddingLeft={4} paddingTop={1} flexDirection="column">
          <text fg={resultFg(theme)}>{resultText}</text>
        </box>
      )}
    </box>
  );
}
