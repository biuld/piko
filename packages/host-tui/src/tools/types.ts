import type { Component } from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";

// ============================================================================
// Tool render types (simplified from pi's ToolDefinition — render only, no execute)
// ============================================================================

/** Context passed to tool render callbacks */
export interface ToolRenderContext<TState = any, TArgs = any> {
  /** Current tool call arguments */
  args: TArgs;
  /** Unique tool call id */
  toolCallId: string;
  /** Invalidate for redraw */
  invalidate: () => void;
  /** Previously returned component for this render slot */
  lastComponent: Component | undefined;
  /** Shared renderer state */
  state: TState;
  /** Working directory */
  cwd: string;
  /** Whether execution has started */
  executionStarted: boolean;
  /** Whether args are complete */
  argsComplete: boolean;
  /** Whether result is partial/streaming */
  isPartial: boolean;
  /** Whether the result view is expanded */
  expanded: boolean;
  /** Whether result is an error */
  isError: boolean;
}

/** Rendering options for tool results */
export interface ToolRenderResultOptions {
  expanded: boolean;
  isPartial: boolean;
}

/** Tool definition for rendering (no execution — engine handles that) */
export interface ToolDef<TArgs = any, TDetails = any, TState = any> {
  /** Tool name */
  name: string;
  /** Custom rendering for tool call display */
  renderCall?: (args: TArgs, theme: Theme, context: ToolRenderContext<TState, TArgs>) => Component;
  /** Custom rendering for tool result display */
  renderResult?: (
    result: { content: any; details?: TDetails },
    options: ToolRenderResultOptions,
    theme: Theme,
    context: ToolRenderContext<TState, TArgs>,
  ) => Component;
  /** "default" = standard colored shell, "self" = tool renders its own framing */
  renderShell?: "default" | "self";
}
