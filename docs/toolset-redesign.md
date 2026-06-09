# piko Engine Protocol Upgrade: ToolSet And AgentOrchestrator

## Status

Design document for implementation.

This document replaces the earlier "add more builtin tools" direction. The new direction is:

1. Delete the current builtin file-operation toolset.
2. Introduce a stable ToolSet API in `engine-protocol`.
3. Introduce an in-memory event-sourced `AgentOrchestrator` between Host and Engine.
4. Keep `StatelessEngine` as the primitive step executor.

This is a protocol-level architecture upgrade, not a small `engine-native/tools` patch.

## Goals

- Replace the current builtin tools:
  - `read`
  - `write`
  - `edit`
  - `bash`
  - `grep`
  - `find`
  - `ls`
- Use a Codex-like default tool surface:
  - `shell`
  - `apply_patch`
  - `update_plan`
  - `view_image`
  - `tool_search`
  - future MCP/dynamic/namespace tools
- Define ToolSet as a first-class grouped capability surface, not just `EngineTool[]`.
- Let different agents receive different ToolSets.
- Add `AgentOrchestrator` as the bridge between Host and one or more stateless Engine runs.
- Support parallel agents, watches, timer wakeups, subagent delegation, and realtime graph rendering.
- Keep Orchestrator state in memory only, using an in-memory event log as the source of truth.

## Non-Goals

- No durable event sourcing.
- No database-backed orchestration.
- No immediate MCP full implementation.
- No compatibility mode for the removed builtin toolset in the default runtime.
- No CRUD-style replacement tools such as `copy`, `move`, `delete`, `stat`, `tree`, `read_many`, or `replace`.
- No multiple `PikoHost` instances for subagents.

## Reference From Codex

Local Codex reference paths:

- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_spec.rs`
- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_executor.rs`
- `/Users/biu/Projects/codex/codex-rs/tools/src/tool_discovery.rs`
- `/Users/biu/Projects/codex/codex-rs/core/src/tools/handlers/mod.rs`
- `/Users/biu/Projects/codex/codex-rs/prompts/templates/apply_patch_tool_instructions.md`
- `/Users/biu/Projects/codex/codex-rs/protocol/src/plan_tool.rs`

Important ideas to carry into piko:

- Tools have exposure modes: direct, deferred, hidden, model-only.
- Tool search/discovery is a first-class capability.
- File editing should be expressed through a patch tool, not many small file mutation tools.
- Shell is the universal workspace inspection/execution primitive.
- Tool definitions and execution policy must not drift apart.
- Runtime orchestration is separate from low-level model/tool step execution.

## Target Architecture

Current architecture:

```text
cli -> host-tui -> host-runtime -> engine-native / engine-remote -> engine-protocol
```

Target architecture:

```text
cli
  -> host-tui
  -> host-runtime
  -> agent-orchestrator
  -> engine-native / engine-remote
  -> engine-protocol
```

Responsibilities:

```text
Host
  owns UI, session persistence, settings, auth, approval UI, resource loading

AgentOrchestrator
  owns agents, tasks, watches, scheduling, locks, per-agent transcript/runtime state,
  fan-out/fan-in, observability, graph projection

StatelessEngine
  owns one agent step: provider call, tool execution, approval pause/resume checkpoint

ToolSet
  owns grouped tool capability definitions and policy
```

The Host remains singular. Subagents are not separate Hosts.

## Package Plan

### `packages/engine-protocol`

Add protocol-level types:

```text
packages/engine-protocol/src/
  engine.ts          existing EngineInput/Event/StepResult
  tools.ts           ToolSet / ToolDefinition / ToolExposure / ToolPolicy
  agents.ts          AgentSpec / AgentTask / AgentRuntimeState
  orchestrator.ts    OrchestratorEvent / State / Graph / Watch
```

`engine.ts` should import/re-export tool types from `tools.ts`.

### `packages/agent-orchestrator`

New package.

```text
packages/agent-orchestrator/src/
  index.ts
  orchestrator.ts
  reducer.ts
  events.ts
  graph.ts
  scheduler.ts
  locks.ts
  watches.ts
  toolsets.ts
  engine-runner.ts
```

Dependencies:

- may depend on `piko-engine-protocol`
- may depend on `piko-engine-native` only in tests or adapters, not core protocol
- must not depend on `host-tui`
- should avoid depending on `host-runtime` except through explicit adapter interfaces

### `packages/host-runtime`

Host integrates Orchestrator:

```text
host-runtime
  creates one AgentOrchestrator
  passes model/auth/settings/toolsets into it
  subscribes to Orchestrator events
  maps Orchestrator events to Host lifecycle/TUI events
```

## ToolSet API

### Why ToolSet Exists

The current `EngineTool[]` is too low-level:

- It does not represent grouped capability surfaces.
- It cannot cleanly support `tool_search`.
- It cannot express agent roles.
- It has no stable policy model.
- It has no direct/deferred/hidden exposure.
- It mixes provider-facing schema with runtime execution concerns.

ToolSet is the source of truth. `EngineTool[]` becomes a provider/runtime projection.

### Core Types

```ts
export interface EngineToolSet {
  id: string;
  name: string;
  description?: string;
  tools: EngineToolDefinition[];
  policy?: ToolSetPolicy;
  metadata?: EngineToolSetMetadata;
}

export interface EngineToolSetMetadata {
  source?: "builtin" | "host" | "mcp" | "plugin" | "dynamic" | "agent";
  tags?: string[];
}

export interface ToolSetPolicy {
  defaultApproval?: ToolApprovalRequirement;
  allowParallel?: boolean;
  requiresWriteLock?: boolean;
  maxConcurrentCalls?: number;
}
```

Tool definitions:

```ts
export interface EngineToolDefinition {
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

export type EngineToolExposure =
  | "direct"
  | "deferred"
  | "hidden"
  | "direct_model_only";

export type ToolApprovalRequirement =
  | "never"
  | "on_request"
  | "always";

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

export interface EngineToolMetadata {
  title?: string;
  readOnly?: boolean;
  destructive?: boolean;
  mutatesWorkspace?: boolean;
  producesArtifact?: boolean;
}
```

Executor refs:

```ts
export interface EngineToolExecutorRef {
  kind: "native" | "host" | "remote" | "sandbox" | "mcp" | "orchestrator";
  target: string;
  extra?: Record<string, unknown>;
}
```

`host` is for Host-mediated UI/session tools such as `update_plan`, `view_image`, and `request_user_input`.

`orchestrator` is for tools that affect agent scheduling, such as `delegate_to_agent`.

### Projection To Current EngineTool

Current `EngineInput.tools?: EngineTool[]` can be retained initially, but it should be a projection from ToolSets.

```ts
export interface EngineInput {
  runId: string;
  stepId: string;
  transcript: Message[];
  systemPrompt: string;
  model: Model<string>;
  provider: EngineProviderConfig;
  toolSets?: EngineToolSet[];
  tools?: EngineToolDefinition[]; // transitional projection
  settings: EngineRunSettings;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}
```

Implementation rule:

- If `toolSets` is provided, project tools from ToolSets.
- If only `tools` is provided, support old tests and transitional callers.
- Long-term callers should use ToolSets.

### Tool Exposure Semantics

Provider-visible:

```ts
function isProviderVisible(tool: EngineToolDefinition): boolean {
  const exposure = tool.exposure ?? "direct";
  return exposure === "direct" || exposure === "direct_model_only";
}
```

Search-visible:

```ts
function isSearchVisible(tool: EngineToolDefinition): boolean {
  return (tool.exposure ?? "direct") === "deferred";
}
```

Executable:

- `direct`: provider-visible and executable
- `direct_model_only`: provider-visible, special case for model-only surfaces
- `deferred`: not provider-visible by default, discoverable via `tool_search`
- `hidden`: not provider-visible and not discoverable; callable only by internal runtime/orchestrator

### ToolSet Examples

Core coding:

```ts
export const coreCodingToolSet: EngineToolSet = {
  id: "builtin:core-coding",
  name: "Core Coding",
  description: "Default coding tools: shell and apply_patch.",
  tools: [
    {
      name: "shell",
      description: "Execute a shell command in the workspace.",
      inputSchema: shellSchema,
      executor: { kind: "native", target: "shell" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["execute_process", "read_workspace", "write_workspace"],
      approval: "always",
    },
    {
      name: "apply_patch",
      description: "Apply a structured patch to files in the workspace.",
      inputSchema: applyPatchSchema,
      executor: { kind: "native", target: "apply_patch" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["write_workspace"],
      approval: "always",
    },
  ],
  policy: {
    requiresWriteLock: true,
  },
};
```

Planning:

```ts
export const planningToolSet: EngineToolSet = {
  id: "builtin:planning",
  name: "Planning",
  tools: [
    {
      name: "update_plan",
      description: "Update the visible task plan.",
      inputSchema: updatePlanSchema,
      executor: { kind: "host", target: "update_plan" },
      exposure: "direct",
      capabilities: ["update_plan"],
      approval: "never",
    },
  ],
};
```

Discovery:

```ts
export const discoveryToolSet: EngineToolSet = {
  id: "builtin:discovery",
  name: "Tool Discovery",
  tools: [
    {
      name: "tool_search",
      description: "Search deferred tools available in this session.",
      inputSchema: toolSearchSchema,
      executor: { kind: "orchestrator", target: "tool_search" },
      exposure: "direct",
      capabilities: ["discover_tools"],
      approval: "never",
    },
  ],
};
```

Agent delegation:

```ts
export const delegationToolSet: EngineToolSet = {
  id: "builtin:delegation",
  name: "Agent Delegation",
  tools: [
    {
      name: "delegate_to_agent",
      description: "Delegate a task to another registered agent.",
      inputSchema: delegateToAgentSchema,
      executor: { kind: "orchestrator", target: "delegate_to_agent" },
      exposure: "direct",
      capabilities: ["delegate_agent"],
      approval: "never",
    },
  ],
};
```

## Replacement Default Tool Surface

The old builtin tools are removed from the default toolset:

- remove `read`
- remove `write`
- remove `edit`
- remove `bash`
- remove `grep`
- remove `find`
- remove `ls`

Replacement:

| Old Tool | Replacement |
|---|---|
| `bash` | `shell` |
| `read` | `shell` with `cat`, `sed`, `rg`, or project commands |
| `grep` | `shell` with `rg` |
| `find` | `shell` with `find` or `rg --files` |
| `ls` | `shell` with `ls` |
| `write` | `apply_patch` |
| `edit` | `apply_patch` |

No legacy alias in the default runtime.

If a test or downstream consumer needs the old tools, implement an explicit non-default package-local helper:

```ts
createLegacyFileToolSet(cwd)
```

Do not call it from `createNativeEngine()` by default.

## New Core Tools

### `shell`

Schema:

```ts
{
  command: string;
  timeout?: number;
  cwd?: string;
  login?: boolean;
}
```

Minimum implementation:

- `command`
- `timeout`
- cwd fixed to workspace root

Output:

```ts
{
  command: string;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
  timedOut: boolean;
  truncated: boolean;
}
```

Policy:

- `approval: "always"`
- `executionMode: "sequential"`
- requires process slot
- may require write lock when command is not known read-only; first version can conservatively require write lock for all shell calls

### `apply_patch`

Schema:

```ts
{
  patch: string;
}
```

Use Codex patch grammar:

```text
*** Begin Patch
*** Add File: path
+content
*** Update File: path
@@
-old
+new
*** Delete File: path
*** End Patch
```

Rules:

- reject absolute paths
- reject paths outside workspace
- reject malformed grammar
- reject ambiguous hunks
- return structured summary

Output:

```ts
{
  applied: boolean;
  filesAdded: string[];
  filesUpdated: string[];
  filesDeleted: string[];
  filesMoved: Array<{ from: string; to: string }>;
  hunksApplied: number;
}
```

Policy:

- `approval: "always"`
- `executionMode: "sequential"`
- requires workspace write lock

### `update_plan`

Schema:

```ts
{
  explanation?: string;
  plan: Array<{
    step: string;
    status: "pending" | "in_progress" | "completed";
  }>;
}
```

Rules:

- at least one plan item
- at most one `in_progress`
- no approval required
- Host/TUI visible state

Executor:

- `kind: "host"`
- target: `update_plan`

### `view_image`

Schema:

```ts
{
  path: string;
  detail?: "high" | "original";
}
```

Rules:

- read-only
- image files only
- no approval required by default
- emits Host/TUI-visible artifact or image lifecycle event

Executor:

- can start as `kind: "host"`
- later may become native if image content blocks are fully engine-owned

### `tool_search`

Schema:

```ts
{
  query: string;
  limit?: number;
}
```

Search target:

- deferred tools in all registered ToolSets
- tool names, descriptions, tags, ToolSet metadata

Output:

```ts
{
  tools: Array<{
    name: string;
    description?: string;
    toolSetId: string;
    toolSetName: string;
    capabilities: EngineToolCapability[];
  }>;
}
```

Executor:

- `kind: "orchestrator"`
- target: `tool_search`

## AgentOrchestrator

### Purpose

`AgentOrchestrator` introduces the agent/team layer without making Host responsible for agent runtime semantics.

It is between Host and Engine:

```text
Host -> AgentOrchestrator -> StatelessEngine
```

It owns:

- agent registry
- agent runtime states
- task queue
- watches and timed wakeups
- scheduling decisions
- workspace locks
- per-agent transcript snapshots
- per-agent `engineState`
- pending approvals
- delegated subagent calls
- event log
- graph projection

It does not own:

- durable session persistence
- TUI rendering
- auth storage
- model registry
- provider implementation
- tool low-level execution

### In-Memory Event Sourcing

The Orchestrator should be in-memory event sourced.

Meaning:

- the in-memory event log is the runtime source of truth
- current state is a reducer projection from events
- graph is a projection from state
- events can be dumped as JSONL for debugging
- no durable replay requirement

Core shape:

```ts
export interface AgentOrchestrator {
  registerAgent(spec: AgentSpec): void;
  unregisterAgent(agentId: string): void;

  dispatch(task: AgentTask): Promise<AgentTaskId>;
  wake(agentId: string, reason: WakeReason): Promise<void>;
  tick(now?: number): Promise<void>;

  registerWatch(watch: AgentWatch): AgentWatchId;
  unregisterWatch(watchId: AgentWatchId): void;

  subscribe(listener: OrchestratorEventListener): () => void;
  snapshot(): OrchestratorState;
  dumpEvents(): OrchestratorEventEnvelope[];
  renderGraph(): OrchestratorGraph;

  start(): void;
  stop(): Promise<void>;
}
```

Implementation rule:

```ts
emit(event) {
  const envelope = withMeta(event);
  this.events.push(envelope);
  this.state = reduce(this.state, envelope);
  this.subscribers.forEach((fn) => fn(envelope, this.state));
}
```

No direct mutation of `state` outside `emit -> reduce`.

### Agent Spec

```ts
export interface AgentSpec {
  id: string;
  name: string;
  role: string;
  description?: string;
  systemPrompt: string;
  model?: string;
  toolSetIds: string[];
  maxSteps?: number;
  concurrency?: AgentConcurrencyPolicy;
}

export interface AgentConcurrencyPolicy {
  canRunInParallel?: boolean;
  requiresWriteLock?: boolean;
  maxConcurrentTasks?: number;
}
```

Example agents:

```ts
const coordinator: AgentSpec = {
  id: "coordinator",
  name: "Coordinator",
  role: "Plans work and delegates to specialist agents.",
  systemPrompt: "Coordinate the team. Do not edit files directly.",
  toolSetIds: ["builtin:planning", "builtin:discovery", "builtin:delegation"],
};

const implementer: AgentSpec = {
  id: "implementer",
  name: "Implementer",
  role: "Makes code changes.",
  systemPrompt: "Implement scoped changes using shell and apply_patch.",
  toolSetIds: ["builtin:core-coding", "builtin:planning"],
  concurrency: { requiresWriteLock: true, maxConcurrentTasks: 1 },
};

const reviewer: AgentSpec = {
  id: "reviewer",
  name: "Reviewer",
  role: "Reviews code and reports issues.",
  systemPrompt: "Review code. Do not mutate files.",
  toolSetIds: ["builtin:read-only-shell"],
  concurrency: { canRunInParallel: true },
};
```

### Agent Runtime State

```ts
export type AgentStatus =
  | "idle"
  | "queued"
  | "running"
  | "blocked"
  | "waiting"
  | "failed";

export interface AgentRuntimeState {
  id: string;
  spec: AgentSpec;
  status: AgentStatus;
  inbox: AgentTaskId[];
  activeTaskId?: AgentTaskId;
  transcript: Message[];
  engineState?: unknown;
  lastWakeReason?: WakeReason;
}
```

### Tasks

```ts
export type AgentTaskId = string;

export interface AgentTask {
  id?: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  priority?: number;
  parentTaskId?: string;
  correlationId?: string;
}

export type TaskSource =
  | { kind: "user" }
  | { kind: "watch"; watchId: string }
  | { kind: "timer"; watchId: string }
  | { kind: "agent"; agentId: string; taskId: string }
  | { kind: "approval"; approvalId: string };

export type AgentTaskStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "blocked"
  | "cancelled";

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

export interface AgentTaskResult {
  summary: string;
  artifacts?: AgentArtifact[];
}
```

### Watches And Wakeups

```ts
export type AgentWatch =
  | {
      kind: "interval";
      id?: string;
      agentId: string;
      everyMs: number;
      prompt: string;
    }
  | {
      kind: "file_change";
      id?: string;
      agentId: string;
      paths: string[];
      debounceMs: number;
      prompt: string;
    }
  | {
      kind: "queue";
      id?: string;
      agentId: string;
      queueName: string;
    }
  | {
      kind: "dependency";
      id?: string;
      agentId: string;
      afterTaskId: string;
      prompt: string;
    };

export type WakeReason =
  | { kind: "user_task"; taskId: string }
  | { kind: "timer"; watchId: string }
  | { kind: "file_change"; watchId: string; paths: string[] }
  | { kind: "subagent_result"; fromAgentId: string; taskId: string }
  | { kind: "approval_resolved"; approvalId: string };
```

Watch triggers enqueue tasks. They do not run agents directly. Every wakeup flows through scheduler and locks.

### Locks

```ts
export type LockMode = "read" | "write";

export interface LockState {
  id: string;
  resource: string;
  mode: LockMode;
  holderAgentId?: string;
  holderTaskId?: string;
  queue: Array<{ agentId: string; taskId: string; mode: LockMode }>;
}
```

Initial lock resources:

- `workspace`
- `process`

Rules:

- reviewer/read-only agents can run in parallel
- implementer agents require workspace write lock
- `apply_patch` always requires workspace write lock
- `shell` can conservatively require workspace write lock in v1
- later classify shell commands as read/write through approval policy

### Orchestrator Events

Events are the in-memory source of truth.

```ts
export interface OrchestratorEventEnvelope {
  meta: OrchestratorEventMeta;
  event: OrchestratorEvent;
}

export interface OrchestratorEventMeta {
  eventId: string;
  timestamp: number;
  orchestratorRunId: string;
  correlationId?: string;
  parentTaskId?: string;
}
```

Event union:

```ts
export type OrchestratorEvent =
  | { type: "orchestrator_started"; runId: string }
  | { type: "orchestrator_stopped"; runId: string; reason?: string }

  | { type: "toolset_registered"; toolSetId: string; name: string }
  | { type: "agent_registered"; agentId: string; name: string; role: string; toolSetIds: string[] }
  | { type: "agent_unregistered"; agentId: string }
  | { type: "agent_status_changed"; agentId: string; from: AgentStatus; to: AgentStatus; reason?: string }

  | { type: "watch_registered"; watchId: string; agentId: string; kind: AgentWatch["kind"] }
  | { type: "watch_unregistered"; watchId: string }
  | { type: "watch_triggered"; watchId: string; agentId: string; reason: WakeReason }

  | { type: "task_enqueued"; task: AgentTaskState }
  | { type: "task_started"; taskId: string; agentId: string }
  | { type: "task_completed"; taskId: string; agentId: string; result: AgentTaskResult }
  | { type: "task_failed"; taskId: string; agentId: string; error: string }
  | { type: "task_blocked"; taskId: string; agentId: string; reason: string }

  | { type: "scheduler_decision"; decision: SchedulerDecision }

  | { type: "lock_requested"; lockId: string; agentId: string; taskId: string; resource: string; mode: LockMode }
  | { type: "lock_acquired"; lockId: string; agentId: string; taskId: string; resource: string; mode: LockMode }
  | { type: "lock_released"; lockId: string; agentId: string; taskId: string; resource: string }

  | { type: "engine_step_started"; agentId: string; taskId: string; stepId: string }
  | { type: "engine_event"; agentId: string; taskId: string; stepId: string; event: EngineEvent }
  | { type: "engine_step_completed"; agentId: string; taskId: string; stepId: string; status: EngineStepStatus }

  | { type: "approval_requested"; agentId: string; taskId: string; approvalId: string; details: unknown }
  | { type: "approval_resolved"; agentId: string; taskId: string; approvalId: string; decision: string }

  | { type: "artifact_produced"; agentId: string; taskId: string; artifact: AgentArtifact };
```

Scheduler decisions must be events:

```ts
export type SchedulerDecision =
  | {
      kind: "started";
      agentId: string;
      taskId: string;
    }
  | {
      kind: "skipped" | "deferred";
      agentId?: string;
      taskId?: string;
      reason:
        | "agent_busy"
        | "lock_unavailable"
        | "priority_lower_than_running"
        | "no_tasks"
        | "rate_limited"
        | "awaiting_approval";
    };
```

This is required for observability. It should be possible to explain why an agent did not pick up work.

### Reducer And State

```ts
export interface OrchestratorState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, EngineToolSet>;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  watches: Record<string, AgentWatchState>;
  locks: Record<string, LockState>;
  approvals: Record<string, ApprovalRuntimeState>;
  artifacts: Record<string, AgentArtifact>;
}
```

Reducer:

```ts
export function reduceOrchestratorEvent(
  state: OrchestratorState,
  envelope: OrchestratorEventEnvelope,
): OrchestratorState;
```

Rules:

- reducer must be deterministic
- no side effects in reducer
- all state transitions must have a corresponding event
- tests should assert event sequence and final projection

### Graph Projection

Graph is derived from `OrchestratorState`, not stored separately.

```ts
export interface OrchestratorGraph {
  nodes: OrchestratorGraphNode[];
  edges: OrchestratorGraphEdge[];
}

export interface OrchestratorGraphNode {
  id: string;
  kind: "agent" | "task" | "watch" | "lock" | "approval" | "artifact";
  status: string;
  label: string;
  metadata?: Record<string, unknown>;
}

export interface OrchestratorGraphEdge {
  from: string;
  to: string;
  kind:
    | "assigned_to"
    | "triggered"
    | "waiting_for"
    | "blocked_by"
    | "spawned"
    | "produced"
    | "requires";
}
```

TUI can render this graph in a future team view.

### Engine Runner

The Orchestrator runs a stateless Engine step for one agent:

```ts
async function runAgentStep(agentId: string, taskId: string): Promise<void> {
  const agent = state.agents[agentId];
  const task = state.tasks[taskId];
  const toolSets = resolveToolSets(agent.spec.toolSetIds);
  const input = buildEngineInput(agent, task, toolSets);

  emit({ type: "engine_step_started", agentId, taskId, stepId: input.stepId });

  const stream = engine.executeStep(input);
  for await (const event of stream) {
    emit({ type: "engine_event", agentId, taskId, stepId: input.stepId, event });
  }

  const result = await stream.result();
  emit({ type: "engine_step_completed", agentId, taskId, stepId: input.stepId, status: result.status });
}
```

Engine remains unaware of agents/teams.

## Delegate Tool

`delegate_to_agent` should be an orchestrator tool.

Schema:

```ts
{
  agentId: string;
  prompt: string;
  priority?: number;
}
```

Behavior:

- called by coordinator agent
- creates `task_enqueued`
- returns task id and current status
- does not block by default

Possible future options:

```ts
{
  waitForResult?: boolean;
  timeoutMs?: number;
}
```

First version should not block; fan-in can be done by watches/dependency tasks.

## Tool Search With ToolSets

`tool_search` should search ToolSet metadata, not a flat random list.

Search entries:

```ts
export interface ToolSearchEntry {
  toolSetId: string;
  toolSetName: string;
  toolName: string;
  description: string;
  capabilities: EngineToolCapability[];
  tags: string[];
  exposure: EngineToolExposure;
}
```

Search result:

```ts
export interface ToolSearchResult {
  tools: ToolSearchEntry[];
}
```

First ranking:

- exact name match
- prefix name match
- description substring
- tag match

No BM25 needed initially.

## Host Integration

Host should own one orchestrator instance.

```ts
class PikoHost {
  private orchestrator: AgentOrchestrator;
}
```

Host responsibilities:

- instantiate orchestrator
- register default agents/toolsets
- subscribe to orchestrator events
- map orchestrator events into Host lifecycle/TUI events
- route approval requests
- route Host-mediated tool calls
- optionally dump orchestrator trace into session metadata if requested

Host should not:

- schedule individual subagent engine steps
- inspect subagent engine internals
- mutate orchestrator state directly

## Default Team Shape

Initial team:

```text
coordinator
  tools: planning + discovery + delegation

implementer
  tools: core-coding + planning
  lock: workspace write

reviewer
  tools: read-only-shell
  lock: none/read

tester
  tools: shell
  lock: process slot
```

This is enough for:

- user asks coordinator
- coordinator updates plan
- coordinator delegates investigation to reviewer
- coordinator delegates edits to implementer
- coordinator delegates verification to tester
- graph shows all tasks and edges

## Migration Plan

### Phase 1: Protocol Types

Files:

- `packages/engine-protocol/src/tools.ts`
- `packages/engine-protocol/src/agents.ts`
- `packages/engine-protocol/src/orchestrator.ts`
- `packages/engine-protocol/src/index.ts`
- `packages/engine-protocol/src/engine.ts`

Tasks:

1. Add ToolSet API.
2. Add Agent API.
3. Add Orchestrator event/state/graph types.
4. Add `toolSets?: EngineToolSet[]` to `EngineInput`.
5. Keep `tools?: EngineToolDefinition[]` during transition.

Acceptance:

- `bun run check`
- Protocol exports all new types.

### Phase 2: ToolSet Projection

Files:

- `packages/engine-native/src/provider-runner.ts`
- `packages/engine-native/src/tool-runner.ts`
- `packages/engine-native/src/tools/registry.ts`

Tasks:

1. Project provider-visible tools from ToolSets.
2. Preserve `tools` fallback for tests.
3. Use typed approval/capability metadata.
4. Add exposure filtering tests.

Acceptance:

- direct tools reach provider
- deferred/hidden tools do not
- old tests still pass through fallback

### Phase 3: New Default ToolSet

Files:

- `packages/engine-native/src/tools/shell.ts`
- `packages/engine-native/src/tools/apply-patch/`
- `packages/engine-native/src/tools/registry.ts`
- `packages/engine-native/src/system-prompt.ts`

Tasks:

1. Add `shell`.
2. Add `apply_patch`.
3. Change `createBuiltinCodingToolSet` default to only new tools.
4. Remove old tools from default registry.
5. Move old tools to explicit test-only or legacy helper if required.
6. Update tests.

Acceptance:

- default native engine exposes no old builtin tools
- all file edits in tests use `apply_patch`
- all exploration in tests uses `shell`

### Phase 4: AgentOrchestrator Package

Files:

- `packages/agent-orchestrator/`
- root `tsconfig.json`
- root `package.json`

Tasks:

1. Create package.
2. Implement event log.
3. Implement reducer.
4. Implement snapshot/dump/renderGraph.
5. Implement agent registry.
6. Implement task queue.
7. Implement basic scheduler.
8. Implement lock manager.
9. Bridge Engine events into Orchestrator events.

Acceptance:

- tests can enqueue a task and observe event sequence
- tests can run two read-only agents in parallel
- tests prevent two write-lock agents from running concurrently
- graph projection contains agents/tasks/locks

### Phase 5: Watches And Wakeups

Tasks:

1. Add interval watches.
2. Add manual `wake`.
3. Add dependency watches.
4. Add file-change watch only if a lightweight watcher is already available; otherwise defer.

Acceptance:

- interval watch enqueues a task on tick
- dependency watch fires after task completion
- scheduler decision events explain skipped tasks

### Phase 6: Host Integration

Tasks:

1. Host creates one orchestrator.
2. Host registers default team/toolsets.
3. Host maps Orchestrator events to lifecycle/TUI events.
4. Host routes approvals.
5. Host exposes graph snapshot for TUI.

Acceptance:

- existing single-agent path still works
- team path can be enabled by setting/flag
- TUI can inspect orchestrator graph snapshot

## Testing Strategy

### Protocol

- ToolSet type construction
- exposure filtering helper
- event type exhaustiveness

### Engine Native

- default tools no longer include old tool names
- `shell` executes command
- `apply_patch` add/update/delete/move
- provider sees only direct tools

### Agent Orchestrator

- register agent
- enqueue task
- scheduler starts runnable task
- scheduler emits skipped/deferred reasons
- lock prevents concurrent writers
- engine event bridge emits wrapped events
- dumpEvents returns append-only event log
- renderGraph derives expected nodes/edges

### Host Runtime

- Host owns one Orchestrator
- Host does not create sub-Hosts
- approval flow includes agent/task context
- lifecycle includes orchestrator event mapping

## Acceptance Criteria

The upgrade is complete when:

1. `EngineToolSet` is the main tool surface in protocol.
2. Default native engine no longer registers `read/write/edit/bash/grep/find/ls`.
3. Default native engine exposes `shell` and `apply_patch`.
4. `update_plan`, `view_image`, `tool_search`, and `delegate_to_agent` have protocol-level tool definitions, even if some are initially Host/Orchestrator-mediated.
5. `AgentOrchestrator` exists as a separate layer between Host and Engine.
6. Orchestrator state is derived from an in-memory event log.
7. Orchestrator can dump events and render a realtime graph projection.
8. Orchestrator supports registering watches and waking agents.
9. Scheduler decision events explain why tasks run or do not run.
10. Host remains single-instance and does not become the subagent runtime.
11. `bun run check` passes.
12. `bun run test` passes.

## Implementation Notes For Deepseek

Work in small commits.

Suggested order:

1. Add protocol types only.
2. Add ToolSet projection and exposure filtering.
3. Add new `shell` and `apply_patch` tools.
4. Remove old tools from default registry.
5. Create `agent-orchestrator` package with event log and reducer.
6. Add scheduler/locks.
7. Add Engine bridge.
8. Add Host integration.
9. Add watches.
10. Add graph rendering projection.

After every phase:

```bash
bun run check
bun run test
```

