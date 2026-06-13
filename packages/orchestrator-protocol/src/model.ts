// ---- Public model config / capability types ----
// Host-visible model types (not the internal ModelStepExecutor subsystem).

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
  maxSteps: number;
  parallelTools?: boolean;
  allowToolCalls: boolean;
  thinkingLevel?: string;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: { stopOnAssistantMessage?: boolean; stopOnToolResult?: boolean };
  runtimeLimits?: ModelRuntimeLimits;
}
