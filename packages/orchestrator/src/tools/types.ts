// ---- Tool & ToolSet type definitions ----
// Moved from piko-protocol/src/tools/

/** Capability tags that describe what a tool can do. */
export type ToolCapability =
  | "read_workspace"
  | "write_workspace"
  | "execute_process"
  | "network"
  | "view_image"
  | "update_plan"
  | "request_user_input"
  | "delegate_agent"
  | "discover_tools";

/** When a tool is visible to the provider and/or searchable. */
export type ToolExposure = "direct" | "deferred" | "hidden" | "direct_model_only";

/** Approval requirement for a tool. */
export type ToolApprovalRequirement = "never" | "on_request" | "always";

/** Executor ref: where the tool implementation lives. */
export interface ToolExecutorRef {
  kind: "native" | "host" | "remote" | "sandbox" | "mcp" | "orchestrator";
  target: string;
  extra?: Record<string, unknown>;
}

/** Metadata for tool documentation and policy. */
export interface ToolMetadata {
  title?: string;
  readOnly?: boolean;
  destructive?: boolean;
  mutatesWorkspace?: boolean;
  producesArtifact?: boolean;
}

/** A single tool definition — the canonical per-tool shape. */
export interface ToolDef {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: ToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  exposure?: ToolExposure;
  capabilities?: ToolCapability[];
  approval?: ToolApprovalRequirement;
  metadata?: ToolMetadata;
}

// ---- ToolSet types (unified) ----

/** ToolSet metadata. */
export interface ToolSetMetadata {
  source?: "builtin" | "host" | "mcp" | "plugin" | "dynamic" | "agent";
  tags?: string[];
}

/** A named, grouped capability surface. Tools are references, not inline definitions. */
export interface ToolSet {
  id: string;
  name: string;
  description?: string;
  tools: ToolSetToolRef[];
  /** Per-tool-set policy defaults (orchestrator level). */
  policy?: ToolSetPolicy;
  metadata?: ToolSetMetadata;
}

export type ToolSetEntry = ToolDef | ToolSetToolRef;

export type ToolSetToolRef = ProviderToolRef | ProviderNamespaceRef | OrchestratorControlRef;

export interface ProviderToolRef {
  kind: "provider_tool";
  providerId: string;
  toolName: string;
  alias?: string;
  policy?: Partial<ToolPolicy>;
}

export interface ProviderNamespaceRef {
  kind: "provider_namespace";
  providerId: string;
  namespace: string;
  alias?: string;
  policy?: Partial<ToolPolicy>;
}

export interface OrchestratorControlRef {
  kind: "orchestrator_control";
  action: string;
  alias?: string;
  policy?: Partial<ToolPolicy>;
}

// ---- Policy types ----

export type ToolSensitivity = "safe" | "sensitive" | "dangerous" | "dynamic";
export type ToolApprovalPolicy = "never" | "on_sensitive" | "always";
export type ToolExecutionMode = "parallel" | "sequential";
export type ToolFailureMode = "return_error" | "fail_task";

export interface ToolPolicy {
  sensitivity?: ToolSensitivity;
  approval?: ToolApprovalPolicy;
  timeoutMs?: number;
  executionMode?: ToolExecutionMode;
  failureMode?: ToolFailureMode;
}

export interface ToolSetPolicy {
  defaults?: Partial<ToolPolicy>;
  allowParallel?: boolean;
  maxConcurrentCalls?: number;
}
