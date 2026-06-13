// ---- ToolProvider protocol types ----
// Shared across orchestrator, engine-native, host-runtime, and future providers.

import type { ToolCall } from "../types.js";
import type { ToolDef } from "./index.js";

// ---- ToolProvider ----

/** Source classification for a tool provider. */
export type ToolProviderSource = "orchestrator" | "host" | "engine" | "mcp" | "plugin";

/** Context passed to provider.discover() to scope tool discovery. */
export interface ToolDiscoveryContext {
  agentId: string;
  taskId?: string;
  toolSetIds: string[];
  activeToolNames?: string[];
}

/** Context for tool execution. */
export interface ToolExecutionContext {
  agentId: string;
  taskId: string;
  toolSetIds: string[];
}

/** Structured tool execution result. */
export interface ToolExecResult {
  ok: boolean;
  value?: unknown;
  error?: { code: string; message: string; retryable?: boolean };
}

/**
 * A ToolProvider is the discovery and execution adapter for one source of tools.
 * ToolActor owns coordination around the provider: approval, lifecycle events,
 * timeout, cancellation, and structured results.
 */
export interface ToolProvider {
  id: string;
  source: ToolProviderSource;

  /** Discover available tools for the given context. */
  discover(context: ToolDiscoveryContext): Promise<ToolDef[]>;

  /** Execute a tool call. */
  execute(call: ToolCall, context: ToolExecutionContext): Promise<ToolExecResult>;
}
