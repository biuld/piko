// ============================================================================
// Tool Renderer — per-tool display formatters
// Replaces raw JSON argument display with readable tool-specific output
// ============================================================================

// ============================================================================
// Types
// ============================================================================

export type ToolStatus = "pending" | "running" | "success" | "error" | "aborted";

export interface ToolDisplayProps {
  name: string;
  args: Record<string, unknown>;
  result?: unknown;
  status: ToolStatus;
  isExpanded: boolean;
  /** Working directory for path-relative display */
  cwd?: string;
}

export interface ToolDisplayOutput {
  /** Single-line summary suitable for collapsed view */
  summary: string;
  /** Detailed output for expanded view */
  details?: string;
  /** Status icon (ASCII-safe) */
  icon: string;
  /** Status color token path (e.g. "text.muted") */
  statusColor: string;
}

// ============================================================================
// Status helpers
// ============================================================================

export function getToolIcon(status: ToolStatus): string {
  switch (status) {
    case "pending":
      return "○";
    case "running":
      return "●";
    case "success":
      return "✓";
    case "error":
      return "✗";
    case "aborted":
      return "⊘";
  }
}

export function getToolStatusColor(status: ToolStatus): string {
  switch (status) {
    case "pending":
      return "text.muted";
    case "running":
      return "text.warning";
    case "success":
      return "text.success";
    case "error":
      return "text.error";
    case "aborted":
      return "text.muted";
  }
}

// ============================================================================
// Per-tool display functions
// ============================================================================

/** Format a file path for display */
function formatPath(path: string, cwd?: string): string {
  if (cwd && path.startsWith(cwd)) {
    return path.slice(cwd.length + 1);
  }
  return path;
}

/** Format number of lines/lines range */
function formatLines(count: number): string {
  return `${count} line${count !== 1 ? "s" : ""}`;
}

/** Format duration in ms */
function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

// ---- read ----

function displayRead(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const path = formatPath(String(args.path ?? ""));
  const lines = typeof result === "string" ? result.split("\n").length : 0;

  let summary = `read ${path}`;
  if (lines > 0) summary += `  (${formatLines(lines)})`;

  let details: string | undefined;
  if (typeof result === "string" && result.length > 0) {
    details = result.length > 1000 ? `${result.slice(0, 1000)}\n...` : result;
  }

  return { summary, details, icon: ">", statusColor: "text.muted" };
}

// ---- bash ----

function displayBash(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const command = String(args.command ?? args.cmd ?? "");
  const exitCode = args.exitCode as number | undefined;
  const duration = args.duration as number | undefined;

  let summary = `$ ${command}`;
  if (exitCode !== undefined) {
    summary += exitCode === 0 ? `  [0]` : `  [${exitCode}]`;
  }
  if (duration) summary += `  ${formatDuration(duration)}`;

  let details: string | undefined;
  if (typeof result === "string" && result.length > 0) {
    details = result.length > 2000 ? `${result.slice(0, 2000)}\n...` : result;
  }

  return { summary, details, icon: "$", statusColor: "text.muted" };
}

// ---- edit / write ----

function displayEdit(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const path = formatPath(String(args.path ?? args.filePath ?? ""));
  const oldText = String(args.oldText ?? "");
  const newText = String(args.newText ?? "");
  const oldLines = oldText.split("\n").length;
  const newLines = newText.split("\n").length;
  const diff = Math.abs(newLines - oldLines);
  const sign = newLines >= oldLines ? "+" : "-";

  const summary = `edit ${path}  ${sign}${diff} lines`;

  let details: string | undefined;
  if (oldText && newText) {
    details = `- ${oldText.slice(0, 200)}\n+ ${newText.slice(0, 200)}`;
    if (oldText.length > 200 || newText.length > 200) details += "\n...";
  } else if (typeof result === "string") {
    details = result.slice(0, 500);
  }

  return { summary, details, icon: "✎", statusColor: "text.muted" };
}

// ---- write (new file) ----

function displayWrite(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const path = formatPath(String(args.path ?? args.filePath ?? ""));
  const content = String(args.content ?? "");
  const lines = content.split("\n").length;

  const summary = `write ${path}  (${formatLines(lines)})`;

  let details: string | undefined;
  if (content.length > 0) {
    details = content.length > 500 ? `${content.slice(0, 500)}\n...` : content;
  } else if (typeof result === "string") {
    details = result.slice(0, 500);
  }

  return { summary, details, icon: "+", statusColor: "text.muted" };
}

// ---- grep ----

function displayGrep(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const pattern = String(args.pattern ?? "");
  const path = args.path ? formatPath(String(args.path)) : "";
  const matchCount = typeof result === "string" ? result.split("\n").filter(Boolean).length : 0;

  let summary = `grep "${pattern}"`;
  if (path) summary += ` ${path}`;
  if (matchCount > 0) summary += `  (${matchCount} match${matchCount !== 1 ? "es" : ""})`;

  let details: string | undefined;
  if (typeof result === "string" && result.length > 0) {
    details = result.length > 1000 ? `${result.slice(0, 1000)}\n...` : result;
  }

  return { summary, details, icon: "?", statusColor: "text.muted" };
}

// ---- find ----

function displayFind(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const pattern = String(args.pattern ?? args.glob ?? "");
  const path = args.path ? formatPath(String(args.path)) : ".";
  const fileCount = typeof result === "string" ? result.split("\n").filter(Boolean).length : 0;

  let summary = `find "${pattern}"`;
  if (path) summary += ` in ${path}`;
  if (fileCount > 0) summary += `  (${fileCount} file${fileCount !== 1 ? "s" : ""})`;

  let details: string | undefined;
  if (typeof result === "string" && result.length > 0) {
    details = result.length > 1000 ? `${result.slice(0, 1000)}\n...` : result;
  }

  return { summary, details, icon: "?", statusColor: "text.muted" };
}

// ---- ls ----

function displayLs(args: Record<string, unknown>, result?: unknown): ToolDisplayOutput {
  const path = args.path ? formatPath(String(args.path)) : ".";
  const entryCount = typeof result === "string" ? result.split("\n").filter(Boolean).length : 0;

  const summary = `ls ${path}${entryCount > 0 ? `  (${entryCount} entries)` : ""}`;

  let details: string | undefined;
  if (typeof result === "string" && result.length > 0) {
    details = result.length > 1000 ? `${result.slice(0, 1000)}\n...` : result;
  }

  return { summary, details, icon: "?", statusColor: "text.muted" };
}

// ============================================================================
// Dispatcher
// ============================================================================

/**
 * Format a tool call for display.
 * Returns human-readable summary and optional details.
 */
export function formatToolDisplay(props: ToolDisplayProps): ToolDisplayOutput {
  const { name, status } = props;

  // Get tool-specific display
  let display: ToolDisplayOutput;

  switch (name) {
    case "read":
      display = displayRead(props.args, props.result);
      break;
    case "bash":
    case "exec":
      display = displayBash(props.args, props.result);
      break;
    case "edit":
      display = displayEdit(props.args, props.result);
      break;
    case "write":
      display = displayWrite(props.args, props.result);
      break;
    case "grep":
      display = displayGrep(props.args, props.result);
      break;
    case "find":
      display = displayFind(props.args, props.result);
      break;
    case "ls":
      display = displayLs(props.args, props.result);
      break;
    default:
      // Generic tool display
      display = {
        summary: `[tool] ${name}`,
        details: props.args ? JSON.stringify(props.args, null, 2).slice(0, 500) : undefined,
        icon: "○",
        statusColor: "text.muted",
      };
  }

  // Override status icon/color
  return {
    ...display,
    icon: getToolIcon(status),
    statusColor: getToolStatusColor(status),
  };
}
