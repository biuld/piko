// ---- Public model config / capability types ----

// ---- Capabilities ----
export interface ToolInfo {
  name: string;
  description: string;
}

export interface ModelCapabilities {
  supportsTools: boolean;
  supportsSandbox: boolean;
  supportsMCP: boolean;
  tools: ToolInfo[];
}

// ---- Provider config ----
export interface ModelProviderConfig {
  apiKey?: string;
  headers?: Record<string, string>;
  reasoning?: { effort?: string; summary?: string };
  sessionId?: string;
  baseUrl?: string;
  extra?: Record<string, unknown>;
}

// ---- Runtime limits ----
export interface ModelRuntimeLimits {
  maxModelCalls?: number;
  maxToolCalls?: number;
  maxWallClockMs?: number;
  maxConsecutiveErrors?: number;
  perToolTimeoutMs?: number;
}

export interface ModelRuntimeCounters {
  modelCalls: number;
  toolCalls: number;
  consecutiveErrors: number;
  startedAt: number;
}

export interface ModelRunSettings {
  parallelTools?: boolean;
  allowToolCalls: boolean;
  thinkingLevel?: string;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: { stopOnAssistantMessage?: boolean; stopOnToolResult?: boolean };
  runtimeLimits?: ModelRuntimeLimits;
  maxTokens?: number;
}

// ---- Model catalog ----
export interface ModelSummary {
  id: string;
  name: string;
  reasoning: boolean;
  input: ("text" | "image")[];
  contextWindow: number;
  maxTokens: number;
}

export interface ProviderInfo {
  provider: string;
  models: ModelSummary[];
}

export interface ResolvedModel {
  provider: string;
  model: ModelSummary;
  providerConfig: ModelProviderConfig;
}
