// ============================================================================
// Layout measurement utilities
// ============================================================================

/**
 * Measure the visible width of a string (stripping ANSI escape codes).
 */
export function visibleWidth(text: string): number {
  // Strip ANSI escape sequences
  const stripped = text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
  return stripped.length;
}

/**
 * Truncate text to fit within a given display width.
 * Preserves ANSI escape codes, truncating only the visible content.
 */
export function truncateToWidth(text: string, maxWidth: number): string {
  if (maxWidth <= 0) return "";
  if (visibleWidth(text) <= maxWidth) return text;

  let result = "";
  let visible = 0;
  let i = 0;

  while (i < text.length && visible < maxWidth) {
    if (text[i] === "\x1b") {
      // Consume the entire escape sequence
      let j = i;
      while (j < text.length && text[j] !== "m") j++;
      result += text.slice(i, j + 1);
      i = j + 1;
    } else {
      result += text[i];
      visible++;
      i++;
    }
  }

  return result;
}

/**
 * Calculate how many lines a text will occupy given a width.
 * Handles newlines and wrapping.
 */
export function measureTextLines(text: string, width: number): number {
  if (width <= 0) return 0;

  const lines = text.split("\n");
  let total = 0;

  for (const line of lines) {
    const vw = visibleWidth(line);
    total += Math.max(1, Math.ceil(vw / Math.max(1, width)));
  }

  // For empty string, split yields [""] → 1, but there's no content → return 0
  return text.length === 0 ? 0 : total;
}

/**
 * Get the terminal dimensions.
 * Uses process.stdout if available, falls back to defaults.
 */
export function getTerminalSize(): { width: number; height: number } {
  if (process.stdout?.rows && process.stdout?.columns) {
    return {
      width: process.stdout.columns,
      height: process.stdout.rows,
    };
  }
  return { width: 80, height: 24 };
}
