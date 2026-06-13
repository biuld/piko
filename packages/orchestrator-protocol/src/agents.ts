// ---- Agent & Task protocol types ----
// Host-visible agent and task types.

import type { Message } from "./messages.js";

// ---- Agent types ----

export type AgentStatus = "idle" | "running" | "failed" | "stopped";

export interface AgentConcurrencyPolicy {
  maxConcurrentTasks?: number;
}

export interface AgentSpec {
  id: string;
  name: string;
  role: string;
  description?: string;
  systemPrompt: string;
  model?: string;
  toolSetIds: string[];
  activeToolNames?: string[];
  maxSteps?: number;
  concurrency?: AgentConcurrencyPolicy;
}

export interface AgentRuntimeState {
  id: string;
  spec: AgentSpec;
  status: AgentStatus;
  activeTaskId?: string;
  transcript: Message[];
}

// ---- Task types ----

export type AgentTaskId = string;

export type TaskSource = { type: "user" } | { type: "agent"; agentId: string; taskId: string };

export type AgentTaskStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export interface AgentTask {
  id?: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  priority?: number;
  parentTaskId?: string;
}

export interface AgentTaskState {
  id: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  status: AgentTaskStatus;
  priority: number;
  parentTaskId?: string;
  result?: AgentTaskResult;
  error?: string;
}

export interface AgentArtifact {
  id: string;
  type: string;
  data: unknown;
}

export interface AgentTaskResult {
  summary: string;
  artifacts?: AgentArtifact[];
}
