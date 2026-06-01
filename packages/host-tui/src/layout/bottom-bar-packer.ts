// ============================================================================
// BottomBarPacker — deterministic packing of status fields by terminal width
// ============================================================================

/**
 * Fields that can appear in the bottom bar.
 * Ordered by priority: earlier fields are kept when width is constrained.
 */
export type BottomBarFieldPacked =
  | "cwd"
  | "model"
  | "tokens"
  | "cost"
  | "context"
  | "session"
  | "branch"
  | "thinking"
  | "hints";

export interface BottomBarInput {
  cwd: string;
  gitBranch?: string;
  sessionName?: string;
  modelProvider: string;
  modelId: string;
  thinkingLevel?: string;
  inputTokens: string;
  outputTokens: string;
  cacheTokens?: string;
  cost: string;
  contextPercent?: string;
  contextWindow?: string;
  messageCount: number;
  hints: string[];
}

export interface BottomBarLines {
  line1: string;
  line2: string;
  /** True when fields were dropped due to width constraints */
  truncated: boolean;
}

/**
 * Pack bottom bar fields into two lines that fit within the given width.
 *
 * Algorithm:
 * 1. Build full line 1: cwd (branch) - session | model
 * 2. Build full line 2: tokens cost context msgs | hints
 * 3. If lines overflow, drop fields from right-to-left within each line,
 *    preserving priority order.
 */
export function packBottomBar(input: BottomBarInput, width: number): BottomBarLines {
  // Build line 1 components
  const leftParts: string[] = [];

  // cwd: always present, abbreviated
  leftParts.push(input.cwd);

  // git branch
  if (input.gitBranch) {
    leftParts.push(`(${input.gitBranch})`);
  }

  // session name
  if (input.sessionName) {
    leftParts.push(`- ${input.sessionName}`);
  }

  const leftStr = leftParts.join(" ");

  // Right side: model
  const rightStr = `${input.modelProvider}/${input.modelId}`;

  // Build line 2 components
  const statsParts: string[] = [];

  if (input.inputTokens) statsParts.push(`↑${input.inputTokens}`);
  if (input.outputTokens) statsParts.push(`↓${input.outputTokens}`);
  if (input.cacheTokens) statsParts.push(`cache ${input.cacheTokens}`);
  if (input.cost) statsParts.push(input.cost);
  if (input.contextPercent && input.contextWindow) {
    statsParts.push(`ctx ${input.contextPercent}/${input.contextWindow}`);
  }
  statsParts.push(`${input.messageCount} msgs`);

  const statsStr = statsParts.join(" ");

  // Hints
  const hintsStr = input.hints.join(" ");

  // Pack line 1: left | right
  const line1 = packLine(leftStr, rightStr, width);

  // Pack line 2: stats | hints
  const line2 = packLine(statsStr, hintsStr, width);

  // Determine if anything was dropped
  const fullLine1 = `${leftStr}  ${rightStr}`;
  const fullLine2 = `${statsStr}  ${hintsStr}`;
  const truncated = visibleLength(fullLine1) > width || visibleLength(fullLine2) > width;

  return { line1, line2, truncated };
}

/**
 * Pack a left string and right string into one line.
 * If they don't both fit, truncate from the right side of the left string.
 * If the right string alone doesn't fit, truncate it.
 */
function packLine(left: string, right: string, width: number): string {
  const leftLen = visibleLength(left);
  const rightLen = visibleLength(right);
  const space = Math.max(0, width - rightLen - 1);

  if (leftLen + rightLen + 1 <= width) {
    // Both fit with a space
    return left + " ".repeat(width - leftLen - rightLen) + right;
  }

  if (rightLen >= width) {
    // Right side alone doesn't fit — truncate it
    return truncateRight(right, width);
  }

  // Left side needs truncation
  const truncatedLeft = truncateRight(left, space);
  const pad = Math.max(1, width - visibleLength(truncatedLeft) - rightLen);
  return truncatedLeft + " ".repeat(pad) + right;
}

// ============================================================================
// Helpers
// ============================================================================

/** Visible length excluding ANSI codes */
export function visibleLength(text: string): number {
  return text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "").length;
}

/** Truncate from the right to fit within maxWidth, preserving ANSI codes */
function truncateRight(text: string, maxWidth: number): string {
  if (maxWidth <= 0) return "";
  if (visibleLength(text) <= maxWidth) return text;

  let result = "";
  let visible = 0;
  let i = 0;

  while (i < text.length && visible < maxWidth) {
    if (text[i] === "\x1b") {
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

/** Middle-truncate a path: keep prefix and suffix, replace middle with ... */
export function middleTruncate(text: string, maxWidth: number): string {
  if (maxWidth <= 0) return "";
  const vlen = visibleLength(text);
  if (vlen <= maxWidth) return text;

  const keepEach = Math.floor((maxWidth - 3) / 2);
  if (keepEach <= 0) return truncateRight(text, maxWidth);

  const prefix = truncateRight(text, keepEach);
  // Get suffix: take last `keepEach` visible characters
  let suffix = "";
  let visible = 0;
  let idx = text.length - 1;
  // Count backwards for visible chars
  while (idx >= 0 && visible < keepEach) {
    if (text[idx] === "m") {
      // Skip back to start of ANSI code
      while (idx >= 0 && text[idx] !== "\x1b") idx--;
    }
    visible++;
    idx--;
  }
  // Extract suffix preserving ANSI
  suffix = text.slice(idx + 1);

  // Handle ANSI codes that span the boundary
  const cleanSuffix = suffix.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");

  return `${prefix}...${cleanSuffix}`;
}
