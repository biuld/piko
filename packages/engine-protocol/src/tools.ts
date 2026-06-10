// ---- Tool & ToolSet type definitions ----

/** Capability tags that describe what a tool can do. */
export type EngineToolCapability =
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
export type EngineToolExposure = "direct" | "deferred" | "hidden" | "direct_model_only";

/** Approval requirement for a tool. */
export type ToolApprovalRequirement = "never" | "on_request" | "always";

/** Executor ref: where the tool implementation lives. */
export interface EngineToolExecutorRef {
  kind: "native" | "host" | "remote" | "sandbox" | "mcp" | "orchestrator";
  target: string;
  extra?: Record<string, unknown>;
}

/** Metadata for tool documentation and policy. */
export interface EngineToolMetadata {
  title?: string;
  readOnly?: boolean;
  destructive?: boolean;
  mutatesWorkspace?: boolean;
  producesArtifact?: boolean;
}

/** A single tool definition — shared by ToolSet and Engine directly. */
export interface EngineTool {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: EngineToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  exposure?: EngineToolExposure;
  capabilities?: EngineToolCapability[];
  approval?: ToolApprovalRequirement;
  metadata?: EngineToolMetadata;
}

/** ToolSet metadata. */
export interface EngineToolSetMetadata {
  source?: "builtin" | "host" | "mcp" | "plugin" | "dynamic" | "agent";
  tags?: string[];
}

/** A named, grouped capability surface. */
export interface EngineToolSet {
  id: string;
  name: string;
  description?: string;
  tools: EngineTool[];
  metadata?: EngineToolSetMetadata;
}

// ---- Exposure helpers ----

export function isProviderVisible(tool: EngineTool): boolean {
  const exposure = tool.exposure ?? "direct";
  return exposure === "direct" || exposure === "direct_model_only";
}

export function isSearchVisible(tool: EngineTool): boolean {
  return (tool.exposure ?? "direct") === "deferred";
}

// ---- Provider projection ----

/** Filter provider-visible tools from a ToolSet list. */
export function projectProviderTools(toolSets: EngineToolSet[]): EngineTool[] {
  return toolSets.flatMap((ts) => ts.tools.filter(isProviderVisible));
}

/** Build a combined registry from toolSets. */
export type ToolSetRegistry = Record<string, (args: Record<string, unknown>) => Promise<unknown>>;

export function projectToolSetRegistry(
  toolSets: EngineToolSet[],
  nativeRegistry: ToolSetRegistry,
): ToolSetRegistry {
  const merged: ToolSetRegistry = { ...nativeRegistry };
  for (const ts of toolSets) {
    for (const tool of ts.tools) {
      if (tool.executor.kind === "native" && !merged[tool.executor.target]) {
        // Native executors must already be registered; skip if missing.
      }
    }
  }
  return merged;
}

// ---- Tool search entry ----

export interface ToolSearchEntry {
  toolSetId: string;
  toolSetName: string;
  toolName: string;
  description: string;
  capabilities: EngineToolCapability[];
  tags: string[];
  exposure: EngineToolExposure;
}

export interface ToolSearchResult {
  tools: ToolSearchEntry[];
}

const _scoreKey = Symbol("score");

type ScoredEntry = ToolSearchEntry & { [_scoreKey]: number };

export function searchToolSets(
  toolSets: EngineToolSet[],
  query: string,
  limit?: number,
): ToolSearchResult {
  const q = query.toLowerCase();
  const entries: ScoredEntry[] = [];

  for (const ts of toolSets) {
    for (const tool of ts.tools) {
      if (!isSearchVisible(tool)) continue;

      const nameLower = tool.name.toLowerCase();
      const descLower = tool.description.toLowerCase();

      let score = 0;
      if (nameLower === q) score = 100;
      else if (nameLower.startsWith(q)) score = 80;
      else if (descLower.includes(q)) score = 50;
      else if ((ts.metadata?.tags ?? []).some((t) => t.toLowerCase().includes(q))) score = 30;

      if (score > 0) {
        entries.push({
          toolSetId: ts.id,
          toolSetName: ts.name,
          toolName: tool.name,
          description: tool.description,
          capabilities: tool.capabilities ?? [],
          tags: ts.metadata?.tags ?? [],
          exposure: tool.exposure ?? "deferred",
          [_scoreKey]: score,
        });
      }
    }
  }

  entries.sort((a, b) => b[_scoreKey] - a[_scoreKey]);
  const trimmed = limit && limit > 0 ? entries.slice(0, limit) : entries;

  return {
    tools: trimmed.map(({ [_scoreKey]: _, ...entry }) => entry),
  };
}
