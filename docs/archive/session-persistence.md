# Session Persistence Design

## Context

Piko has completed the pi compatibility phase. The current persistence layer is
still modeled as one pi-style conversation branch:

- `SessionTreeEntry` is a tree of entries with one current `leaf`.
- `message` entries do not carry `agentId` or `taskId`.
- `Session.buildContext()` turns the current branch into one linear model
  transcript.
- `HostAgentState` persists only the `main` agent. Subagents are ephemeral and
  return work through `toolResult` messages in the main transcript.
- Orchestrator has agent/task runtime state, but that state is not durable.

For first-class delegated work, each agent needs its own durable transcript, but
the existing pi session format should remain usable as-is. The durable model
should therefore be:

- one pi-compatible JSONL per agent transcript
- one Piko sidecar manifest/event log that links those JSONLs into one session
  persistence graph

## Goals

- Keep every agent transcript pi-compatible.
- Preserve the existing main session JSONL and current main transcript APIs.
- Persist agent/task relationships outside transcript files.
- Keep Orchestrator independent from session storage.
- Allow replay of delegation trees, per-agent transcripts, and task timelines.
- Support future branch-aware views and delegated-agent compaction.

## Non-Goals

- Do not make Orchestrator depend on `SessionManager`.
- Do not change the pi JSONL header or core entry types.
- Do not require old sessions to be migrated before they can be opened.
- Do not put subagent transcripts into the default main model context.
- Do not make all subagents long-lived memory owners by default.

## Storage Layout

The existing main session file stays unchanged:

```text
<timestamp>_<rootSessionId>.jsonl
```

Piko adds a sibling sidecar manifest:

```text
<timestamp>_<rootSessionId>.piko.jsonl
```

Piko stores non-main agent sessions under a sibling directory:

```text
<timestamp>_<rootSessionId>.piko/
  agents/
    <agentId>/
      <timestamp>_<agentSessionId>.jsonl
```

The main agent's session is the root `.jsonl`. Every other agent session is its
own pi-compatible JSONL file. The sidecar does not store the transcript; it
stores relationships and indexes.

If the sidecar is missing, Piko opens the root `.jsonl` as an ordinary
single-agent pi session.

## Why Per-Agent JSONL

Per-agent JSONL is better than putting all agent messages in a single sidecar:

- Every transcript remains readable by the existing pi session parser.
- Each agent has independent leaf, branch, fork, and compaction semantics.
- Concurrent agents do not fight over one branch leaf.
- Agent history can be loaded directly from that agent's JSONL when policy
  allows it.
- The relationship model can evolve without changing transcript
  storage.

The root session remains the canonical user-facing conversation. Subagent
sessions are attached durable transcripts, not standalone sessions in the normal
session picker unless the UI explicitly exposes them.

## Sidecar Manifest

The first sidecar line is a header:

```ts
interface PikoSessionSidecarHeader {
  type: "piko_session_persistence";
  version: 1;
  rootSessionId: string;
  rootSessionPath: string;
  createdAt: string;
}
```

Subsequent lines are append-only records.

### Agent Registry

```ts
interface AgentSessionRecord {
  schema: "piko.agent_session.v1";
  rootSessionId: string;
  agentId: string;
  agentSessionId: string;
  sessionPath: string;
  kind: "main" | "subagent";
  displayName?: string;
  role?: string;
  persistence: "session" | "ephemeral";
  createdAt: string;
}
```

Rules:

- The `main` agent record points at the root `.jsonl`.
- Every persistent non-main agent has one or more agent session records.
- An agent may have multiple session records over time if it is reset, forked,
  or policy creates a fresh transcript per task.

### Task Registry

```ts
interface AgentTaskRecord {
  schema: "piko.agent_task.v1";
  rootSessionId: string;
  taskId: string;
  agentId: string;
  agentSessionId: string;
  parentTaskId?: string;
  sourceAgentId?: string;
  sourceTaskId?: string;
  promptEntryId?: string;
  anchorEntryId?: string;
  status: "queued" | "running" | "completed" | "failed" | "cancelled";
  createdAt: string;
  completedAt?: string;
  summary?: string;
  error?: string;
}
```

`taskId` is the execution thread id. `agentSessionId` tells Piko which JSONL
contains that task's transcript.

### Tool And Approval Events

```ts
interface AgentRuntimeEventRecord {
  schema: "piko.agent_runtime_event.v1";
  rootSessionId: string;
  eventId: string;
  taskId: string;
  agentId: string;
  agentSessionId: string;
  anchorEntryId?: string;
  timestamp: string;
  event:
    | { type: "tool_started"; callId: string; name: string; args?: unknown }
    | { type: "tool_finished"; callId: string; name: string; result: unknown; isError: boolean }
    | { type: "approval_requested"; approvalId: string; toolName: string; toolArgs: unknown }
    | { type: "approval_resolved"; approvalId: string; decision: "accept" | "decline" };
}
```

Tool and approval events are relationship/observability facts. The actual model
messages and tool results that belong in an agent's context are still written to
that agent's JSONL.

## Association Model

The stable association graph is:

```text
rootSessionId
  agentId
    agentSessionId
      taskId
        transcript entries in that agent JSONL
```

Relationship edges:

```text
parentTaskId       task that spawned this task
sourceAgentId      agent that requested the task
sourceTaskId       task that requested the task
anchorEntryId      root/main entry that owns this task for branch/fork views
promptEntryId      entry in the agent JSONL containing the task prompt
```

The sidecar owns these relationships. The transcript JSONLs do not need Piko
fields to describe them.

## Main Transcript Bridge

The root `.jsonl` should not embed full subagent transcripts. It should store a
small bridge object in the ordinary delegation `toolResult`:

```ts
{
  delegated: true,
  taskId: "task_...",
  targetAgentId: "reviewer",
  agentSessionId: "01...",
  mode: "call",
  summary: "Review completed."
}
```

The `taskId` and `agentSessionId` are expansion keys for TUI/export views. The
full transcript lives in the referenced agent JSONL.

## Agent Session Policies

Agents should have explicit persistence policy:

```ts
type AgentPersistencePolicy =
  | { kind: "ephemeral" }
  | {
      kind: "session";
      transcript: "agent_reused" | "task_scoped";
      context: "empty" | "agent_history" | "task_thread";
    };
```

Defaults:

- `main`: `{ kind: "session", transcript: "agent_reused", context:
  "agent_history" }`
- delegated subagents: `{ kind: "session", transcript: "task_scoped", context:
  "empty" }`

`task_scoped` is the safer default: each delegated task gets a fresh agent JSONL
so unrelated subagent tasks do not accidentally share memory. Named persistent
agents can opt into `agent_reused`.

## Projections

Existing main APIs remain:

```ts
loadMessages(): Promise<Message[]>
loadBranchEntries(): Promise<SessionTreeEntry[]>
getBranch(): Promise<SessionTreeEntry[]>
```

New APIs load the sidecar plus relevant agent JSONLs:

```ts
loadAgentSession(agentSessionId: string): Promise<SessionManager>
loadAgentMessages(agentId: string, options?: { taskId?: string }): Promise<Message[]>
loadTaskTranscript(taskId: string): Promise<Message[]>
loadTaskTree(): Promise<SessionTaskNode[]>
loadRuntimeEvents(taskId: string): Promise<AgentRuntimeEventRecord[]>
```

Projection rules:

- Missing sidecar means only `main` exists.
- The root `.jsonl` is `agentId: "main"`.
- Sidecar agent records map non-main `agentId` values to agent JSONLs.
- Task transcript lookup resolves `taskId -> agentSessionId -> JSONL branch`.
- Main model context remains `buildContext(rootBranch)` unless a feature
  explicitly opts into richer context.

## Host Responsibilities

The Host is the persistence boundary.

Host should:

- Create agent JSONLs according to agent persistence policy.
- Register agent JSONLs in the sidecar.
- Subscribe to Orchestrator events and append task/runtime records to the
  sidecar.
- Persist each completed task's transcript into that task's agent JSONL.
- Keep the existing main transcript saving path working.

Host should not:

- Reach into actor private state during a run.
- Make Orchestrator aware of sessions.
- Treat the sidecar as scheduling state.

## Persistence Flow

For `main`:

1. Load history from the root session JSONL.
2. Run Orchestrator.
3. Save final messages to the root session JSONL.
4. Append task/runtime metadata to the sidecar when useful.

For delegated subagents:

1. Determine policy for `targetAgentId`.
2. Create or reuse an agent JSONL.
3. Append `AgentSessionRecord` if this agent JSONL is new.
4. Append/update `AgentTaskRecord` as the task moves through lifecycle states.
5. Run Orchestrator with policy-selected history.
6. Save final messages to the agent JSONL.
7. Return a small bridge result to the parent task/main transcript.

The current HostEvent stream does not include final transcript messages for
subagent tasks. The Orchestrator/Host boundary needs one of these additions:

- Add `messages?: Message[]` to task completion events.
- Add a dedicated `task_transcript_committed` HostEvent.
- Add a Host-side query API `getTaskTranscript(taskId)` after completion.

Prefer `task_transcript_committed`: it makes transcript persistence explicit
without bloating all completion events.

## Lifecycle

The sidecar and agent JSONLs are owned by `SessionManager`/Host runtime, not by
the lower-level pi session parser.

Operations:

- **Create**: create the root `.jsonl` first. Lazily create sidecar and agent
  JSONLs on first delegated-agent task.
- **Open**: open root `.jsonl`; if a matching sidecar exists, load agent
  registry records. Missing sidecar is valid.
- **Rename**: rename root session metadata only. Sidecar and agent JSONLs are
  discovered by root session id and adjacent path.
- **Import**: import root `.jsonl`; import sidecar and agent JSONLs if they are
  adjacent. Otherwise import as single-agent.
- **Delete**: delete root `.jsonl`, sidecar, and attached agent JSONLs.
- **Clone/Fork**: create a new root session id, copy the root branch, then copy
  only attached agent JSONLs/tasks whose anchors belong to the copied branch.

## Branching And Forking

Each agent JSONL has its own branch model. The root branch remains the default
session branch.

`anchorEntryId` links a delegated task back to the root/main branch. Preferred
anchor order:

1. The main assistant message containing the `delegate_to_agent` tool call.
2. The main `toolResult` message that reports the delegated `taskId`.
3. The nearest main user message for the turn if neither entry id is available.

Branch-aware projections:

- Show subagent tasks only when `anchorEntryId` is on the selected root branch.
- Fork only agent sessions/tasks anchored to the copied branch.
- Keep unanchored records available in debug/all-events views, but hide them from
  normal branch replay.

For agent-local branching, use that agent JSONL's own entry ids. The sidecar
should record which agent session belongs to a task, not duplicate the agent's
branch tree.

## Compaction

Compaction stays local to a transcript:

- Root compaction uses the existing main JSONL behavior.
- Agent compaction runs against an agent JSONL.
- Task tree compaction can create a summary in the parent/root transcript while
  keeping the detailed agent JSONL attached.

Do not include full subagent transcripts in the default main context unless they
are explicitly summarized into a main-visible entry.

## TUI Implications

New views can be built from sidecar projections:

- Agent/session selector for attached agent JSONLs.
- Task tree panel showing delegation parent-child relationships.
- Expandable subagent transcript under a `delegate_to_agent` tool result.
- Tool/approval timeline scoped to an agent task.
- Branch replay that filters attached agent sessions by `anchorEntryId`.

The default timeline can remain main-first so existing interaction patterns do
not get noisier.

## Migration

No migration is required for old sessions.

Compatibility rules:

- Missing sidecar means single-agent root session.
- Missing agent JSONL for a sidecar record means that agent transcript is
  unavailable, but the root transcript still opens.
- Malformed sidecar records should be ignored or surfaced as diagnostics; they
  must not make the root session unreadable.
- The pi session header version remains unchanged. Sidecar schema has its own
  version.

## Implementation Plan

### Phase 1: Agent JSONL And Manifest

1. Add sidecar storage helpers for:
   - `piko.agent_session.v1`
   - `piko.agent_task.v1`
   - `piko.agent_runtime_event.v1`
2. Add `SessionManager` APIs:
   - `createAgentSession(agentId, policy)`
   - `openAgentSession(agentSessionId)`
   - `appendAgentTask(record)`
   - `updateAgentTaskStatus(taskId, status, details?)`
   - `loadTaskTranscript(taskId)`
   - `loadTaskTree()`
3. Persist direct `host.run(prompt, agentId)` runs into that agent's JSONL when
   `agentId !== "main"`.
4. Keep `loadMessages()` and `saveMessages()` unchanged for root/main.
5. Add tests proving:
   - old root sessions open without sidecar
   - each subagent transcript is a valid JSONL session
   - sidecar loss does not corrupt root session
   - root context does not include subagent messages

### Phase 2: Orchestrator Delegation Persistence

1. Create/reuse agent sessions for `delegate_to_agent`.
2. Persist task lifecycle into the sidecar.
3. Persist completed subagent transcripts into agent JSONLs.
4. Return bridge metadata (`taskId`, `agentSessionId`) in parent tool results.

### Phase 3: Branch-Aware Agent Views

1. Make `anchorEntryId` mandatory for delegated task records.
2. Teach fork/clone to copy attached agent sessions by anchor.
3. Add TUI expandable delegation transcript.
4. Add task tree and agent transcript panels.

### Phase 4: Delegated-Agent Compaction

1. Add agent-local compaction.
2. Add delegation subtree summaries.
3. Decide when summaries should become main-visible context.

## Open Questions

- Should delegated tasks default to `task_scoped` transcripts permanently, or
  should named agents default to `agent_reused`?
- Should subagent transcripts be persisted incrementally for crash recovery, or
  only committed at task completion?
- Should agent JSONLs be listed in normal session lists, hidden by default, or
  exposed only through the root session?
- Should `anchorEntryId` point at the assistant tool call entry or the resulting
  toolResult entry once both are available?
- How should import/export package root JSONL, sidecar, and attached agent
  JSONLs together?

## Recommended First Cut

Implement per-agent JSONL with a sidecar manifest:

- Root `.jsonl` remains the main agent transcript.
- Each persistent subagent task writes to its own pi-compatible agent JSONL.
- Sidecar records map `taskId -> agentSessionId` and capture delegation
  ancestry.
- Main transcript only stores bridge metadata in the delegation `toolResult`.

This preserves pi compatibility at the transcript level while giving Piko a
real session persistence graph.
