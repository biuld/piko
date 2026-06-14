# ToolSets

`ToolSet` is the capability boundary assigned to an agent. `AgentActor` does not
discover every available tool by default; it calls `ToolRegistry.discoverTools()`
for the tools allowed by its `AgentSpec.toolSetIds`.

```ts
export interface AgentSpec {
  id: string;
  name: string;
  role: string;
  systemPrompt: string;
  model?: string;
  toolSetIds: string[];
  maxSteps?: number;
  concurrency?: AgentConcurrencyPolicy;
}
```

A ToolSet groups provider-backed tools and default policy:

```ts
export interface ToolSet {
  id: string;
  name: string;
  description?: string;
  tools: ToolSetToolRef[];
  policy?: ToolSetPolicy;
  metadata?: ToolSetMetadata;
}

export type ToolSetToolRef =
  | {
      kind: "provider_tool";
      providerId: string;
      toolName: string;
      alias?: string;
      policy?: Partial<ToolPolicy>;
    }
  | {
      kind: "provider_namespace";
      providerId: string;
      namespace: string;
      policy?: Partial<ToolPolicy>;
    }
  | {
      kind: "orchestrator_control";
      action: ActorControlAction;
      alias?: string;
      policy?: Partial<ToolPolicy>;
    };

export interface ToolSetPolicy {
  defaults?: Partial<ToolPolicy>;
  allowParallel?: boolean;
  maxConcurrentCalls?: number;
}

export interface ToolPolicy {
  sensitivity?: "safe" | "sensitive" | "dangerous" | "dynamic";
  approval?: "never" | "on_sensitive" | "always";
  timeoutMs?: number;
  executionMode?: "parallel" | "sequential";
  failureMode?: "return_error" | "fail_task";
}

export interface ToolSetMetadata {
  source?: "builtin" | "host" | "project" | "plugin" | "dynamic";
  tags?: string[];
}
```

`ToolSet` is not a provider. Providers expose possible tools; ToolSets select
which of those tools an agent may use and add policy.

```text
ToolProvider -> discovers available tools
ToolSet      -> selects/aliases/policies tools for an agent
AgentSpec    -> lists toolSetIds
ToolRegistry -> computes final catalog for one model step
```

Example:

```ts
const implementerToolSet: ToolSet = {
  id: "builtin:implementer",
  name: "Implementer",
  tools: [
    { kind: "provider_tool", providerId: "workspace", toolName: "bash" },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "edit",
      policy: { sensitivity: "sensitive", approval: "on_sensitive" },
    },
    { kind: "orchestrator_control", action: "delegate_to_agent" },
    { kind: "orchestrator_control", action: "join_subtask" },
    { kind: "orchestrator_control", action: "update_plan" },
  ],
  policy: {
    defaults: {
      sensitivity: "safe",
      approval: "on_sensitive",
      failureMode: "return_error",
    },
  },
};
```

Prefer narrow per-tool sensitivity overrides over broad ToolSet defaults.
