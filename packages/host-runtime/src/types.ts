/**
 * Type re-exports — bridge from pi-agent-core's internal "../../types.ts"
 * to piko's equivalents.
 *
 * AgentMessage and CustomAgentMessages are now provided by piko-session.
 */

import type { AssistantMessage } from "piko-orch-protocol";
import type { TSchema } from "typebox";

// ============================================================================
// Re-exports from piko-session
// ============================================================================

export type { AgentMessage } from "piko-session";

// ============================================================================
// Core types (from pi-agent-core types.ts)
// ============================================================================

/** Thinking / reasoning level. */
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";

/** Queue / steering mode. */
export type QueueMode = "all" | "one-at-a-time";

/** A single tool call content block. */
export type AgentToolCall = Extract<AssistantMessage["content"][number], { type: "toolCall" }>;

export interface AgentToolResult<T> {
  content: Array<{ type: "text"; text: string }>;
  details?: T;
  isError: boolean;
}

export type AgentToolUpdateCallback<T = any> = (partialResult: AgentToolResult<T>) => void;

export interface AgentTool<TParameters extends TSchema = TSchema, TDetails = any> {
  name: string;
  description: string;
  parameters: TParameters;
  execute: (
    args: any,
    update?: AgentToolUpdateCallback<TDetails>,
  ) => Promise<AgentToolResult<TDetails>>;
  executionMode?: "sequential" | "parallel";
  metadata?: Record<string, unknown>;
}

// AgentEvent is a large union type used by harness-types.ts but not by compaction.
// Define as minimal stub — harness-types.ts references it in event map types.
export type AgentEvent = { type: string };
