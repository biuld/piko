// ============================================================================
// ToolTimelineItem — render tool calls/results, pi-aligned
//
// Pi pattern:
//   - Background color conveys status:
//       toolPendingBg   (running / pending)
//       toolSuccessBg   (completed successfully)
//       toolErrorBg     (error / aborted)
//   - Compact call line with tool-specific formatting
//   - Expanded result with output display
//   - No status icons — status conveyed by color alone
//   - Expand hint: "(Ctrl+O to expand)" / "▲" when expanded
// ============================================================================

import { TextAttributes } from "@opentui/core";
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
      if (offset !== undefined && limit !== undefined && limit > 0) {
        return `read ${path} (L${offset + 1}-${offset + limit})`;
      }
      return `read ${path}`;
    }
    case "write": {
      const path = str(args.file_path ?? args.path);
      const content = str(args.content);
      const lines = content ? content.split("\n").length : 0;
      if (lines > 0) return `write ${path}  (${lines} line${lines !== 1 ? "s" : ""})`;
      return `write ${path}`;
    }
    case "edit": {
      const path = str(args.file_path ?? args.path);
      const oldStr = str(args.oldText ?? "");
      const newStr = str(args.newText ?? "");
      const oldLines = oldStr ? oldStr.split("\n").length : 0;
      const newLines = newStr ? newStr.split("\n").length : 0;
      if (oldLines > 0 && newLines > 0) {
        const diff = newLines - oldLines;
        const sign = diff >= 0 ? "+" : "";
        return `edit ${path}  ${sign}${diff} lines`;
      }
      return `edit ${path}`;
    }
    case "bash":
    case "exec": {
      const cmd = str(args.command ?? args.cmd);
      if (cmd) return `$ ${cmd}`;
      return "bash";
    }
    case "grep": {
      const pattern = str(args.pattern);
      if (pattern) return `grep "${pattern}"`;
      return "grep";
    }
    case "find": {
      const pattern = str(args.pattern ?? args.glob ?? "");
      const dir = str(args.path ?? args.directory);
      if (pattern && dir) return `find "${pattern}" in ${dir}`;
      if (pattern) return `find "${pattern}"`;
      return "find";
    }
    case "ls": {
      const dir = str(args.path ?? args.directory ?? ".");
      return `ls ${dir}`;
    }
    case "web_search":
      return `web_search ${str(args.query ?? "")}`;
    case "web_fetch":
      return `web_fetch ${str(args.url ?? "")}`;
    default:
      return toolName;
  }
}

// ============================================================================
// Background color
// ============================================================================

function toolBg(
  theme: ReturnType<typeof useTheme>,
  status?: string,
  isError?: boolean,
): string {
  if (isError || status === "error") return theme.color("surface.toolError");
  if (status === "running" || status === "pending") return theme.color("surface.toolPending");
  if (status === "success") return theme.color("surface.toolSuccess");
  return theme.color("surface.toolPending");
}

// ============================================================================
// Foreground color for call text (title)
// Pi always uses toolTitle — status conveyed by bg color, not text color.
// ============================================================================

function callFg(theme: ReturnType<typeof useTheme>): string {
  return theme.color("tool.title");
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

function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

// ============================================================================
// Component
// ============================================================================

export function ToolTimelineItem(props: ToolTimelineItemProps) {
  const theme = useTheme();
  const { item, isExpanded } = props;

  const isError =
    item.toolStatus === "error" ||
    (item.toolResult != null &&
      typeof item.toolResult === "object" &&
      (item.toolResult as any)?.isError === true);

  const bgColor = toolBg(theme, item.toolStatus, isError);

  // ---- Compact call line ----
  const callText = formatToolArgs(item);
  const duration = item.toolDuration
    ? `  ${formatDurationMs(item.toolDuration)}`
    : "";

  // ---- Expand hint ----
  const hasResult = item.toolResult != null;
  const expandHint =
    hasResult && !isExpanded ? `  (Ctrl+O to expand)` : "";
  const collapseHint =
    hasResult && isExpanded ? "  ▲" : "";

  // ---- Result (only when expanded) ----
  const showResult = isExpanded && hasResult;
  let resultText = "";
  if (showResult) {
    resultText =
      typeof item.toolResult === "string"
        ? item.toolResult
        : JSON.stringify(item.toolResult, null, 2);
    // Truncate very long results
    if (resultText.length > 4000) {
      resultText = resultText.slice(0, 4000) + "\n... (truncated)";
    }
  }

  // ---- Exit code display ----
  const exitCode = item.toolExitCode;
  const exitCodeText =
    exitCode !== undefined && exitCode !== null && exitCode !== 0
      ? `  [${exitCode}]`
      : exitCode === 0
        ? `  [0]`
        : "";

  return (
    <box flexDirection="column">
      <box height={1} />
      <box
        backgroundColor={bgColor}
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        flexDirection="column"
      >
      {/* Call line */}
      <box flexDirection="row" height={hasResult ? 1 : undefined}>
        <text
          fg={callFg(theme)}
          attributes={TextAttributes.BOLD}
        >
          {callText}{exitCodeText}{duration}
        </text>
        {hasResult && !isExpanded && (
          <text fg={theme.color("text.dim")}>
            {expandHint}
          </text>
        )}
        {hasResult && isExpanded && (
          <text fg={theme.color("text.dim")}>{collapseHint}</text>
        )}
      </box>

      {/* Expanded result */}
      {showResult && resultText && (
        <box paddingLeft={2} paddingTop={1} flexDirection="column">
          <text fg={theme.color("tool.output")}>{resultText}</text>
        </box>
      )}

      {/* Error result without text */}
      {showResult && !resultText && isError && (
        <box paddingLeft={2} paddingTop={1} flexDirection="column">
          <text fg={theme.color("text.error")}>Error</text>
        </box>
      )}
      </box>
    </box>
  );
}
