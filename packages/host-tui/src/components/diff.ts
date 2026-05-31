/**
 * Diff renderer — renders unified diffs with theme-colored lines.
 *
 * Handles:
 * - +++ / --- header lines
 * - @@ hunk headers
 * - + added lines (green)
 * - - removed lines (red)
 * - Context lines (dim/gray)
 */

import { getTheme } from "../theme.js";

export interface RenderDiffOptions {
  /** File path (for display only). */
  filePath?: string;
  /** Maximum lines to render (default: 80). */
  maxLines?: number;
}

/**
 * Parse a unified diff line.
 * Returns the prefix (+, -, space) and content, or null if not a diff line.
 */
function parseDiffLine(line: string): { prefix: string; content: string } | null {
  const match = line.match(/^([+\-\s])(.*)$/);
  if (!match) return null;
  return { prefix: match[1], content: match[2] };
}

/**
 * Render a unified diff string with theme colors.
 */
export function renderDiff(diffText: string, options: RenderDiffOptions = {}): string {
  const theme = getTheme();
  const maxLines = options.maxLines ?? 80;

  const lines = diffText.split("\n");
  const result: string[] = [];

  let lineCount = 0;
  let truncated = false;

  for (const line of lines) {
    if (lineCount >= maxLines) {
      truncated = true;
      break;
    }

    const parsed = parseDiffLine(line);

    if (!parsed) {
      // Header or hunk
      if (line.startsWith("+++") || line.startsWith("---")) {
        result.push(theme.bold(theme.fg("dim", line)));
      } else if (line.startsWith("@@")) {
        result.push(theme.fg("accent", line));
      } else {
        result.push(theme.fg("toolDiffContext", line));
      }
      lineCount++;
      continue;
    }

    switch (parsed.prefix) {
      case "+":
        result.push(theme.fg("toolDiffAdded", line));
        break;
      case "-":
        result.push(theme.fg("toolDiffRemoved", line));
        break;
      default:
        result.push(theme.fg("toolDiffContext", line));
        break;
    }
    lineCount++;
  }

  if (truncated) {
    result.push(theme.fg("muted", `  ... (${lines.length - maxLines} more lines)`));
  }

  return result.join("\n");
}
