// ============================================================================
// Layout measurement utilities
// ============================================================================

/**
 * Measure the visible width of a string (stripping ANSI escape codes).
 */
export function visibleWidth(text: string): number {
  // Strip ANSI escape sequences
  const stripped = text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
  let width = 0;
  for (let i = 0; i < stripped.length; ) {
    const codePoint = stripped.codePointAt(i);
    if (codePoint === undefined) break;
    width += codePointWidth(codePoint);
    i += codePoint > 0xffff ? 2 : 1;
  }
  return width;
}

function codePointWidth(codePoint: number): number {
  if (codePoint === 0) return 0;
  if (codePoint < 32 || (codePoint >= 0x7f && codePoint < 0xa0)) return 0;
  if (isCombiningCodePoint(codePoint)) return 0;
  if (isWideCodePoint(codePoint)) return 2;
  return 1;
}

function isCombiningCodePoint(codePoint: number): boolean {
  return (
    (codePoint >= 0x0300 && codePoint <= 0x036f) ||
    (codePoint >= 0x1ab0 && codePoint <= 0x1aff) ||
    (codePoint >= 0x1dc0 && codePoint <= 0x1dff) ||
    (codePoint >= 0x20d0 && codePoint <= 0x20ff) ||
    (codePoint >= 0xfe00 && codePoint <= 0xfe0f) ||
    (codePoint >= 0xfe20 && codePoint <= 0xfe2f) ||
    codePoint === 0x200d
  );
}

function isWideCodePoint(codePoint: number): boolean {
  return (
    codePoint >= 0x1100 &&
    (codePoint <= 0x115f ||
      codePoint === 0x2329 ||
      codePoint === 0x232a ||
      (codePoint >= 0x2e80 && codePoint <= 0xa4cf && codePoint !== 0x303f) ||
      (codePoint >= 0xac00 && codePoint <= 0xd7a3) ||
      (codePoint >= 0xf900 && codePoint <= 0xfaff) ||
      (codePoint >= 0xfe10 && codePoint <= 0xfe19) ||
      (codePoint >= 0xfe30 && codePoint <= 0xfe6f) ||
      (codePoint >= 0xff00 && codePoint <= 0xff60) ||
      (codePoint >= 0xffe0 && codePoint <= 0xffe6) ||
      (codePoint >= 0x1f300 && codePoint <= 0x1f64f) ||
      (codePoint >= 0x1f900 && codePoint <= 0x1f9ff) ||
      (codePoint >= 0x20000 && codePoint <= 0x3fffd))
  );
}

/**
 * Truncate text to fit within a given display width.
 * Preserves ANSI escape codes, truncating only the visible content.
 * When ellipsis is provided and text is truncated, it replaces the last
 * N visible characters (where N = visibleWidth(ellipsis)).
 */
export function truncateToWidth(text: string, maxWidth: number, ellipsis?: string): string {
  if (maxWidth <= 0) return "";
  if (visibleWidth(text) <= maxWidth) return text;

  // With ellipsis: reserve space, truncate shorter, then append ellipsis
  if (ellipsis) {
    const ellipsisWidth = visibleWidth(ellipsis);
    const contentMax = Math.max(0, maxWidth - ellipsisWidth);
    if (contentMax <= 0) return ellipsis.slice(0, maxWidth);
    const base = truncateToWidth(text, contentMax);
    return base + ellipsis;
  }

  let result = "";
  let visible = 0;
  let i = 0;

  while (i < text.length && visible < maxWidth) {
    if (text[i] === "\x1b") {
      // Consume the entire escape sequence
      let j = i;
      while (j < text.length && !/[A-Za-z~]/.test(text[j] ?? "")) j++;
      result += text.slice(i, j + 1);
      i = j + 1;
    } else {
      const codePoint = text.codePointAt(i);
      if (codePoint === undefined) break;
      const char = String.fromCodePoint(codePoint);
      const width = codePointWidth(codePoint);
      if (width > 0 && visible + width > maxWidth) break;
      result += char;
      visible += width;
      i += codePoint > 0xffff ? 2 : 1;
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
