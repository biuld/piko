// ============================================================================
// TUI Selectors — derived data from state
// ============================================================================

import { getSeverityIcon, isNotificationExpired } from "../notifications/notification-selectors.js";
import type { BottomBarDensity, BottomBarField, LayoutMode, TuiState } from "./state.js";

// ============================================================================
// Layout selectors
// ============================================================================

/**
 * Determine layout mode from viewport dimensions.
 *
 * - regular: width >= 100 and height >= 24
 * - compact: width >= 60 and height >= 16
 * - minimal: everything else
 */
export function selectLayoutMode(state: TuiState): LayoutMode {
  const { width, height } = state.layout.viewport;
  if (width >= 100 && height >= 24) return "regular";
  if (width >= 60 && height >= 16) return "compact";
  return "minimal";
}

/**
 * Determine bottom bar density from viewport dimensions.
 */
export function selectBottomBarDensity(state: TuiState): BottomBarDensity {
  const { width } = state.layout.viewport;
  if (width >= 120) return "full";
  if (width >= 80) return "compact";
  return "minimal";
}

/**
 * Determine visible bottom bar fields based on density and state.
 * In minimal mode, only show the most critical fields.
 */
export function selectBottomBarFields(state: TuiState): BottomBarField[] {
  const density = selectBottomBarDensity(state);

  switch (density) {
    case "full":
      return ["model", "session", "branch", "tokens", "cost", "cwd", "hints"];
    case "compact":
      return ["model", "tokens", "cost", "cwd", "hints"];
    case "minimal":
      return ["model", "cwd"];
  }
}

// ============================================================================
// Chat selectors
// ============================================================================

/**
 * Get visible messages (respecting collapsed tool calls, etc.)
 */
export function selectVisibleMessages(state: TuiState) {
  return state.transcript.filter((msg) => {
    if (msg.role === "tool") return true;
    return true;
  });
}

/**
 * Get the last message index for scroll-to-bottom logic.
 */
export function selectLastMessageIndex(state: TuiState): number {
  return state.transcript.length - 1;
}

// ============================================================================
// Status selectors
// ============================================================================

/**
 * Compose a status line text from stream state and extension slots.
 * Returns an array of status entries.
 */
export function selectStatusEntries(state: TuiState): string[] {
  const entries: string[] = [];

  // Stream state
  if (state.stream.status === "running") {
    if (state.stream.thinkingActive) {
      entries.push("Thinking...");
    }
    if (state.stream.currentToolName) {
      entries.push(`Running ${state.stream.currentToolName}...`);
    }
  }

  // Queue info
  if (state.stream.queueInfo) {
    entries.push(state.stream.queueInfo);
  }

  // Latest unexpired notification
  const now = Date.now();
  for (const n of state.notifications) {
    if (!n.readAt && !isNotificationExpired(n, now)) {
      entries.push(`${getSeverityIcon(n.severity)} ${n.message}`);
      break;
    }
  }

  // Extension status slots are now shown in the bottom bar (matching pi-mono).

  return entries;
}

// ============================================================================
// Usage selectors
// ============================================================================

/**
 * Format token count for display.
 */
export function selectFormattedInputTokens(state: TuiState): string {
  return formatTokens(state.usage.inputTokens);
}

export function selectFormattedOutputTokens(state: TuiState): string {
  return formatTokens(state.usage.outputTokens);
}

export function selectFormattedCost(state: TuiState): string {
  if (state.usage.totalCost <= 0) return "";
  return `$${state.usage.totalCost.toFixed(3)}`;
}

export function selectContextInfo(state: TuiState): string {
  // Fallback to model's context window (matches pi-mono).
  const window = state.usage.contextWindow || state.model.current.contextWindow || 0;
  if (!window) return "";
  if (state.usage.contextPercent === undefined || state.usage.contextPercent === null) {
    return `?/${formatTokens(window)}`;
  }
  return `${state.usage.contextPercent.toFixed(1)}%/${formatTokens(window)}`;
}

// ============================================================================
// Helpers
// ============================================================================

function formatTokens(count: number): string {
  if (count <= 0) return "";
  if (count < 1000) return count.toString();
  if (count < 10_000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  if (count < 10_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  return `${Math.round(count / 1_000_000)}M`;
}
