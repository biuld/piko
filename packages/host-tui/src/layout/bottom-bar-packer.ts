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
  | "thinking";

export interface BottomBarInput {
  cwd: string;
  gitBranch?: string;
  sessionName?: string;
  modelProvider: string;
  modelId: string;
  thinkingLevel?: string;
  inputTokens: string;
  outputTokens: string;
  cacheReadTokens?: string;
  cacheWriteTokens?: string;
  cacheHitRate?: string;
  cost: string;
  contextPercent?: string;
  contextWindow?: string;
  autoCompact?: boolean;
  messageCount: number;
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
 * 1. Line 1: cwd (branch) • session
 * 2. Line 2: usage/context on the left, model/thinking on the right
 * 3. If line 2 overflows, keep usage first and truncate model.
 */
export function packBottomBar(input: BottomBarInput, width: number): BottomBarLines {
  const pwdParts = [input.cwd];
  if (input.gitBranch) {
    pwdParts.push(`(${input.gitBranch})`);
  }
  if (input.sessionName) {
    pwdParts.push(`• ${input.sessionName}`);
  }
  const line1 = truncateRight(pwdParts.join(" "), width);

  const statsParts: string[] = [];
  if (input.inputTokens) statsParts.push(`↑${input.inputTokens}`);
  if (input.outputTokens) statsParts.push(`↓${input.outputTokens}`);
  if (input.cacheReadTokens) statsParts.push(`R${input.cacheReadTokens}`);
  if (input.cacheWriteTokens) statsParts.push(`W${input.cacheWriteTokens}`);
  if (input.cacheHitRate) statsParts.push(`CH${input.cacheHitRate}`);
  if (input.cost) statsParts.push(input.cost);
  if (input.contextPercent && input.contextWindow) {
    const autoSuffix = input.autoCompact ? " (auto)" : "";
    statsParts.push(`${input.contextPercent}/${input.contextWindow}${autoSuffix}`);
  }
  const statsStr = statsParts.join(" ");

  const modelParts = [];
  if (input.modelProvider) {
    modelParts.push(`(${input.modelProvider})`);
  }
  modelParts.push(input.modelId || "no-model");
  if (input.thinkingLevel && input.thinkingLevel !== "off") {
    modelParts.push(`• ${input.thinkingLevel}`);
  } else if (input.thinkingLevel === "off") {
    modelParts.push("• thinking off");
  }
  const modelStr = modelParts.join(" ");

  const line2 = packLine(statsStr, modelStr, width);

  const fullLine1 = pwdParts.join(" ");
  const fullLine2 = `${statsStr}  ${modelStr}`;
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
  const minPadding = 2;

  if (!left) {
    return truncateRight(right, width);
  }

  if (!right) {
    return truncateRight(left, width);
  }

  if (leftLen + minPadding + rightLen <= width) {
    return left + " ".repeat(width - leftLen - rightLen) + right;
  }

  if (leftLen >= width) {
    return truncateRight(left, width);
  }

  const availableForRight = width - leftLen - minPadding;
  if (availableForRight > 0) {
    const truncatedRight = truncateRight(right, availableForRight);
    return (
      left +
      " ".repeat(Math.max(minPadding, width - leftLen - visibleLength(truncatedRight))) +
      truncatedRight
    );
  }

  return truncateRight(left, width);
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
