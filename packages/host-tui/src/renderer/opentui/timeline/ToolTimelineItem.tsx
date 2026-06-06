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

interface ToolHeaderParts {
  title: string;
  args?: string;
  warning?: string;
}

function spanStyle(fg: string, extra?: Record<string, unknown>) {
  return { fg, ...extra } as any;
}

function formatToolHeaderParts(item: TimelineItem): ToolHeaderParts {
  const toolName = normalizeToolName(item.toolName);
  const args = (item.toolArgs ?? {}) as Record<string, unknown>;

  switch (toolName) {
    case "read": {
      const path = str(args.file_path ?? args.path);
      const offset = num(args.offset);
      const limit = num(args.limit);
      const range =
        offset !== undefined && limit !== undefined && limit > 0
          ? `:L${offset + 1}-${offset + limit}`
          : undefined;
      return { title: "read", args: path, warning: range };
    }
    case "write": {
      const path = str(args.file_path ?? args.path);
      const content = str(args.content);
      const lines = content ? content.split("\n").length : 0;
      const suffix = lines > 0 ? `  (${lines} line${lines !== 1 ? "s" : ""})` : "";
      return { title: "write", args: `${path}${suffix}` };
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
        return { title: "edit", args: `${path}  ${sign}${diff} lines` };
      }
      return { title: "edit", args: path };
    }
    case "bash":
    case "exec": {
      const cmd = str(args.command ?? args.cmd);
      return { title: cmd ? `$ ${cmd}` : "bash" };
    }
    case "grep": {
      const pattern = str(args.pattern);
      return { title: "grep", args: pattern ? `"${pattern}"` : undefined };
    }
    case "find": {
      const pattern = str(args.pattern ?? args.glob ?? "");
      const dir = str(args.path ?? args.directory);
      if (pattern && dir) return { title: "find", args: `"${pattern}" in ${dir}` };
      if (pattern) return { title: "find", args: `"${pattern}"` };
      return { title: "find" };
    }
    case "ls": {
      const dir = str(args.path ?? args.directory ?? ".");
      return { title: "ls", args: dir };
    }
    case "web_search":
      return { title: "web_search", args: str(args.query ?? "") };
    case "web_fetch":
      return { title: "web_fetch", args: str(args.url ?? "") };
    default:
      return { title: toolName };
  }
}

function formatToolArgs(item: TimelineItem): string {
  const parts = formatToolHeaderParts(item);
  return `${parts.title}${parts.args ? ` ${parts.args}` : ""}${parts.warning ?? ""}`;
}

function normalizeToolName(name: string | undefined): string {
  if (!name) return "tool";
  const trimmed = name.trim();
  if (!trimmed) return "tool";
  if (trimmed.length > 40 || /[\n\r]/.test(trimmed)) return "tool";
  return trimmed;
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

function normalizeToolOutput(value: unknown): string {
  const raw = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  const lines = raw.replace(/\r/g, "").split("\n");

  while (lines.length > 0 && lines[0].trim() === "") lines.shift();
  while (lines.length > 0 && lines[lines.length - 1].trim() === "") lines.pop();

  const compacted: string[] = [];
  let previousBlank = false;
  for (const line of lines) {
    const isBlank = line.trim() === "";
    if (isBlank && previousBlank) continue;
    compacted.push(line);
    previousBlank = isBlank;
  }

  return compacted.join("\n");
}

// ============================================================================
// Component
// ============================================================================

export function ToolTimelineItem(props: ToolTimelineItemProps) {
  const theme = useTheme();

  const isError = () =>
    props.item.toolStatus === "error" ||
    (props.item.toolResult != null &&
      typeof props.item.toolResult === "object" &&
      (props.item.toolResult as any)?.isError === true);

  const bgColor = () => toolBg(theme, props.item.toolStatus, isError());

  // ---- Compact call line ----
  const headerParts = () => formatToolHeaderParts(props.item);
  const duration = () => props.item.toolDuration
    ? `  ${formatDurationMs(props.item.toolDuration)}`
    : "";

  // ---- Expand hint ----
  const hasResult = () => props.item.toolResult != null;
  const expandHint = () =>
    hasResult() && !props.isExpanded ? `  (Ctrl+O to expand)` : "";
  const collapseHint = () =>
    hasResult() && props.isExpanded ? `  (Ctrl+O to collapse)` : "";

  // ---- Result (only when expanded) ----
  const showResult = () => hasResult() && !props.isCollapsed;
  const resultText = () => {
    if (!showResult()) return "";
    let rText = normalizeToolOutput(props.item.toolResult);
    // Truncate very long results
    if (rText.length > 4000) {
      rText = rText.slice(0, 4000) + "\n... (truncated)";
    }
    return rText;
  };

  // ---- Exit code display ----
  const exitCodeText = () => {
    const exitCode = props.item.toolExitCode;
    return exitCode !== undefined && exitCode !== null && exitCode !== 0
      ? `  [${exitCode}]`
      : exitCode === 0
        ? `  [0]`
        : "";
  };
  const headerText = () => {
    const hint = hasResult()
      ? props.isCollapsed
        ? expandHint()
        : collapseHint()
      : "";
    return `▸ ${formatToolArgs(props.item)}${exitCodeText()}${duration()}${hint}`;
  };

  return (
    <box flexDirection="column">
      <box height={1} />
      <box
        backgroundColor={bgColor()}
        paddingLeft={1}
        paddingRight={1}
        paddingTop={1}
        paddingBottom={1}
        flexDirection="column"
      >
      {/* Call line */}
      <box flexDirection="row" height={1}>
        <text fg={theme.color("tool.title")}>
          <span style={spanStyle(theme.color("text.dim"))}>▸ </span>
          <span style={spanStyle(theme.color("tool.title"), { bold: true })}>{headerParts().title}</span>
          {headerParts().args ? (
            <span style={spanStyle(theme.color("text.accent"))}> {headerParts().args}</span>
          ) : null}
          {headerParts().warning ? (
            <span style={spanStyle(theme.color("text.warning"))}>{headerParts().warning}</span>
          ) : null}
          {exitCodeText() ? (
            <span style={spanStyle(theme.color(isError() ? "text.error" : "text.dim"))}>{exitCodeText()}</span>
          ) : null}
          {duration() ? <span style={spanStyle(theme.color("tool.duration"))}>{duration()}</span> : null}
          {hasResult() && props.isCollapsed ? (
            <>
              <span style={spanStyle(theme.color("text.muted"))}>  (</span>
              <span style={spanStyle(theme.color("text.dim"))}>Ctrl+O</span>
              <span style={spanStyle(theme.color("text.muted"))}> to expand)</span>
            </>
          ) : null}
          {hasResult() && !props.isCollapsed ? (
            <>
              <span style={spanStyle(theme.color("text.muted"))}>  (</span>
              <span style={spanStyle(theme.color("text.dim"))}>Ctrl+O</span>
              <span style={spanStyle(theme.color("text.muted"))}> to collapse)</span>
            </>
          ) : null}
        </text>
      </box>

      {/* Expanded result */}
      {showResult() && resultText() ? (
        <box paddingLeft={2} paddingTop={1} flexDirection="column">
          <text fg={theme.color("tool.output")}>{resultText()}</text>
        </box>
      ) : null}

      {/* Error result without text */}
      {showResult() && !resultText() && isError() ? (
        <box paddingLeft={2} paddingTop={1} flexDirection="column">
          <text fg={theme.color("text.error")}>Error</text>
        </box>
      ) : null}
      </box>
    </box>
  );
}
