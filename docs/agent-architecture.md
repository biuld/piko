# Agent Architecture

本文档是 piko agent 系统的架构规范，覆盖 agent template 配置、加载、运行时实例、多 agent 编排、持久化、恢复和 TUI 展示。字段身份定义以 `docs/agent-identity.md` 为准。

---

## 1. Scope

Agent 系统由两层组成：

- **Agent template**：静态能力定义，主键为 `agent_id`，由 hostd 加载并传给 orchd。
- **Agent task instance**：一次运行时执行，主键为 `task_id`，由 orchd 创建 lifecycle event，hostd 持久化并投影给 TUI。

`agent_id` 不能作为运行时节点主键；`task_id` 才是 task DAG、agent panel、steering、resume restore 的节点 identity。

---

## 2. Template Configuration

hostd owns the agent template registry.

Built-in templates are TOML resources:

```text
packages/hostd/resources/agents/
  main.toml
  general.toml
  scout.toml
  coder.toml
```

Workspace templates are TOML files:

```text
.piko/agents/*.toml
```

The file stem is the `agent_id`. For example:

```text
.piko/agents/reviewer.toml -> agent_id = "reviewer"
```

`main` is the fixed root-turn template id. `general` is the default delegated-task template id for spawn calls that omit `agent_id`.

### TOML Schema

```toml
name = "Scout"
role = "researcher"
description = "Expert at searching the web and summarizing documentation."
system_prompt = "You are Scout, a specialized web researcher..."
tool_set_ids = ["builtin", "web"]
model = { provider = "anthropic", modelId = "claude-3-5-sonnet-20241022" }
thinking_level = "medium"
active_tool_names = ["read", "web_search"]
```

Fields:

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | Display name. Not identity. |
| `role` | yes | Short role label for UI and tool schema descriptions. |
| `description` | no | Capability description used for discovery. |
| `system_prompt` | yes | Template system prompt. |
| `tool_set_ids` | no | Tool sets enabled for this template. Defaults to hostd agent loader defaults. |
| `model` | no | Template-level model override. Missing means inherit session/global model config. |
| `thinking_level` | no | Template-level thinking override. Missing means inherit session/global thinking config. |
| `active_tool_names` | no | Template-level allow-list within enabled tool sets. |

### Loading Rules

1. hostd loads built-in TOML resources first.
2. hostd loads global user templates from `~/.piko/agents/*.toml`.
3. hostd loads workspace templates from `.piko/agents/*.toml`.
4. Later sources override earlier sources with the same `agent_id`.
5. The merged registry must contain `main` and `general`.
6. The merged `HashMap<agent_id, AgentSpec>` is passed into `OrchdConfig`.

Malformed templates are skipped with diagnostics. A malformed workspace template must not corrupt the built-in registry.

---

## 3. Discovery and Selection

`AgentSpecList` returns the merged template registry. It is a template list, not a runtime task list.

The spawn tools expose available templates through their `agent_id` schema description. Omitted spawn `agent_id` means `general`.

Selection rules:

- Root turns always select `main`.
- Explicit spawn `agent_id` selects that template.
- Missing/empty spawn `agent_id` resolves to `general`.
- Runtime must not synthesize `subagent`, `generic`, or an empty id.
- `name` is never accepted as a routing key.

---

## 4. Runtime Model

Every root turn and every spawn creates an agent task instance.

Root task:

```text
task_id = generated root task id
agent_id = "main"
parent_task_id = None
source_agent_id = None
```

Spawned task:

```text
task_id = generated child task id
agent_id = selected template id, default "general"
parent_task_id = parent task_id
source_agent_id = parent agent_id
```

The task DAG is authoritative for runtime relationships. Parent/child hierarchy is never inferred from `agent_id`, `name`, JSONL shard name, or event order.

---

## 5. Orchd Execution

orchd receives the merged `AgentSpec` registry from hostd. Each agent task instance runs through `agent_loop`.

For a task instance, orchd uses:

- `task_id` for runtime identity.
- `agent_id` to fetch `AgentSpec`.
- `AgentSpec.system_prompt` as the agent persona.
- `AgentSpec.model` / inherited model config for model selection.
- `AgentSpec.tool_set_ids` and `active_tool_names` for tool availability.
- `parent_task_id` and `source_agent_id` for task lifecycle metadata.

Spawn tools create child task instances:

| Tool | Parent behavior | Child behavior | Return |
|---|---|---|---|
| `spawn_detached` | Create child and continue | Child runs independently | `{ task_id, status: "detached" }` |
| `spawn` | Create child and wait for terminal report | Child runs independently | Child report as tool result |
| `poll_task` | Query task result | No child behavior change | Report or not-ready |
| `steer_task` | Send steering to `task_id` | Child consumes steering next step | Delivery status |

---

## 6. Event Contract

Events tied to agent runtime must carry both:

```text
task_id
agent_id
```

`task_id` routes to the runtime instance. `agent_id` groups by template and allows hostd/TUI to resolve display labels.

Task creation event:

```rust
TaskEvent::Created {
    session_id,
    task_id,
    agent_id,
    parent_task_id,
    source_agent_id,
    prompt,
    turn_id,
    timestamp,
}
```

Persist events commit transcript facts:

```rust
PersistEvent::Finalized { session_id, message_id, task_id, agent_id, message }
PersistEvent::ToolCallCommitted { session_id, message_id, task_id, agent_id, parent_message_id, message }
PersistEvent::ToolResultCommitted { session_id, message_id, task_id, agent_id, message }
PersistEvent::TaskLifecycle(task_event)
```

Display events are live rendering inputs. They must also include `task_id` and `agent_id`, but they are not the transcript/resume source of truth.

---

## 7. Hostd Runtime State

hostd is authoritative for user-visible agent state:

- template registry loaded from TOML resources/files.
- task DAG from `TaskEvent`.
- runtime agent projection for TUI.
- per-task live view store.
- session storage and snapshots.

Hostd projections:

| Projection | Key | Source |
|---|---|---|
| Agent template registry | `agent_id` | TOML resources/files |
| Task DAG | `task_id` | `TaskEvent` |
| Agent panel rows | `task_id` | task DAG + `AgentSpec` display fields |
| Per-task view store | `task_id` | display/lifecycle/interaction events |
| Foreground subscribed view | `task_id` | one concrete runtime task view |

`AgentConnected` / `AgentDisconnected` are projections of task lifecycle. They are not independent sources of truth.

---

## 8. Persistence

Session storage is a directory:

```text
~/.piko/sessions/<encoded-cwd>/<session-id>/
  main.jsonl
  <agent-id>.jsonl
  tasks.json
```

### JSONL Transcript Shards

`main.jsonl` stores session header, user/root transcript, and session metadata.

`<agent-id>.jsonl` stores committed transcript entries produced by task instances that reference that template id. The file name is only a shard key.

Message/tool entries written by an agent task must include:

```text
agent_id
task_id
```

This is required because the same `agent_id` may be spawned multiple times in the same session.

### `tasks.json`

`tasks.json` is the durable task DAG sidecar. It is keyed by `task_id`.

Each task record stores:

```text
task_id
agent_id
parent_task_id
source_agent_id
prompt
status
summary/error
updated_at
```

`tasks.json` is the persistence source for runtime hierarchy. JSONL shard names are not used to infer parent/child relationships.

---

## 9. Resume

Resume reconstructs the session from committed storage, not from live display deltas.

Restore order:

1. Read `main.jsonl` header for `session_id`, `cwd`, creation metadata.
2. Read `main.jsonl` and all `<agent-id>.jsonl` shards.
3. Merge entries by timestamp / stable sequence.
4. Read `tasks.json`.
5. Rebuild task DAG keyed by `task_id`.
6. Rebuild runtime agent projection (`AgentInfo`) from task DAG and template registry.
7. Rebuild per-task materialized views from committed transcript entries and task metadata.
8. Return `SessionSnapshot` to TUI.

Restored state must satisfy:

- `AgentList` after resume equals the task DAG projection.
- Completed/failed/cancelled task instances remain visible unless explicitly pruned.
- `AgentSubscribe { task_id }` after resume returns that task's restored view.
- Display deltas are not required for resume correctness.

---

## 10. TUI Behavior

TUI views are projections of hostd state.

Agent panel:

- row key: `task_id`
- row label: `AgentSpec.name` / `AgentInfo.name`, with `agent_id` available as a stable template label
- indentation: `parent_task_id`
- status: task status

Duplicate labels are valid. For example, spawning `scout` twice creates two rows with different `task_id` values and the same `agent_id`.

Agent-related commands:

| Command | Meaning |
|---|---|
| `AgentSpecList` | List templates keyed by `agent_id` |
| `AgentList` | List runtime task instances keyed by `task_id` |
| `AgentSubscribe { task_id }` | Foreground one concrete runtime task view |
| `QueueSteer { task_id, message }` | Address a specific runtime task instance |

TUI must not infer hierarchy from display labels, template ids, or JSONL shard names.

---

## 11. Non-Negotiable Invariants

- `agent_id` is template identity.
- `name` is display text only.
- `task_id` is runtime identity.
- `parent_task_id` is the only runtime tree edge.
- `source_agent_id` is metadata, not hierarchy.
- `main` is the fixed root-turn template id.
- `general` is the default delegated-task template id for omitted spawn `agent_id`.
- `subagent`, `generic`, and empty string are not implicit default agent ids.
- Persistent transcript entries for agent output include both `agent_id` and `task_id`.
- Resume reconstructs runtime relationships from `tasks.json`.
