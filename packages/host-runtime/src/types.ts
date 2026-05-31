/**
 * Type re-exports — bridge from pi-agent-core's internal "../../types.ts"
 * to piko's equivalents. This file allows copied pi-agent-core code to
 * compile without modification to their import paths.
 *
 * AgentMessage = Message (from pi-ai, re-exported by piko-engine-protocol).
 * CustomAgentMessages is empty by default (no declaration merging needed).
 */

import type { AssistantMessage, Message as PiMessage } from "@earendil-works/pi-ai";
import type { TSchema } from "typebox";

// ============================================================================
// Core types (from pi-agent-core types.ts)
// ============================================================================

/** Thinking / reasoning level. */
export type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";

/** Stream function used by the agent loop. */
export type StreamFn = (
  model: any,
  context: any,
  options: any,
) =>
  | ReturnType<typeof import("@earendil-works/pi-ai").streamSimple>
  | Promise<ReturnType<typeof import("@earendil-works/pi-ai").streamSimple>>;

/** AgentMessage: union of pi-ai Message + custom messages. */
export interface CustomAgentMessages {
  bashExecution: import("./compaction/messages.js").BashExecutionMessage;
  custom: import("./compaction/messages.js").CustomMessage;
  branchSummary: import("./compaction/messages.js").BranchSummaryMessage;
  compactionSummary: import("./compaction/messages.js").CompactionSummaryMessage;
}

export type AgentMessage = PiMessage | CustomAgentMessages[keyof CustomAgentMessages];

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
