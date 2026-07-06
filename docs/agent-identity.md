# Agent Identity

本文档是 piko 中 agent/task identity 的权威定义。完整 agent 架构见 `docs/agent-architecture.md`。所有协议 DTO、storage、TUI agent panel、orchd supervisor、hostd session state 都必须遵守这里的术语和字段语义。

---

## 1. 概念分层

piko 中的 agent 不是单一实体，必须区分静态 template 和运行时 instance：

| Concept | Primary key | Owner | Meaning |
|---|---|---|---|
| Agent spec/template | `agent_id` | hostd | 静态能力定义：name、role、description、system prompt、model/tool 配置 |
| Agent task instance | `task_id` | hostd task DAG | 一次运行时执行实例，引用一个 `agent_id` |
| Runtime tree edge | `parent_task_id` | hostd task DAG | task instance 之间的父子关系 |
| Spawn source spec | `source_agent_id` | task lifecycle metadata | 创建 child task 的父 agent template id |
| Display name | `name` | `AgentSpec` | UI 展示名，不是 identity |

`agent_id` 和 `task_id` 不可互换。`agent_id` 选择能力模板；`task_id` 标识一次执行。

---

## 2. Field Definitions

### `agent_id`

`agent_id` 是 `AgentSpec` 的稳定主键，也是 `.piko/agents/<agent_id>.toml` 的文件 stem。

Rules:

- `agent_id` identifies a template, not a running node.
- `agent_id` is stable across sessions when the same template exists.
- Multiple task instances may reference the same `agent_id` in one session.
- Root turns always use `main`.
- Spawn calls that omit `agent_id` resolve to `general`.
- Runtime must not materialize omitted spawn selection as `subagent`, `generic`, or an empty string.
- `agent_id` is safe for grouping and filtering by template.
- `agent_id` is not safe for addressing a specific running task.

Examples:

```text
main
general
scout
coder
reviewer
```

### `name`

`name` is the display name from `AgentSpec`.

Rules:

- `name` is user-facing display text only.
- `name` is not unique.
- `name` can change without changing identity.
- No storage, protocol routing, subscription, steering, or task lookup may key by `name`.
- Do not introduce a separate `agent_name` identity field. Use `AgentSpec.name` for display.

### `task_id`

`task_id` is the primary key of a runtime agent task instance.

Rules:

- 一个 Task 是一个 Agent 的运行时实例，且是长期生存的（Long-lived），直到被显式关闭。
- 每一个长期生存的 Agent 运行时实例（如主会话的 `main` 任务，或 spawn 的子任务）都拥有唯一的 `task_id`。后续的多个交互回合（Turn）都必须关联在该同一个 `task_id` 上。
- `task_id` is the node id in the runtime task DAG.
- TUI agent panel rows are keyed by `task_id`.
- Steering, cancellation, polling, task result lookup, per-task view replay, and resume restoration address runtime work by `task_id`.
- A session may contain many `task_id` values that all reference the same `agent_id` (e.g. multiple spawned subagents of the same template).

### `parent_task_id`

`parent_task_id` points to the parent runtime task instance in the same session.

Rules:

- Root task has `parent_task_id = None`.
- Child task created by `spawn` or `spawn_detached` has `parent_task_id = Some(parent task_id)`.
- Runtime agent tree indentation and parent/child display are derived only from `parent_task_id`.
- Parent relationship must not be inferred from `agent_id`, `name`, JSONL file name, or creation order.

### `source_agent_id`

`source_agent_id` records which agent template initiated a child task.

Rules:

- `source_agent_id` is metadata for audit and display.
- `source_agent_id` is not a tree edge.
- The runtime source node is `parent_task_id`.

---

## 3. Agent Template Registry

hostd owns the agent template registry.

Built-in templates are TOML resources under:

```text
packages/hostd/resources/agents/*.toml
```

Workspace templates are TOML files under:

```text
.piko/agents/*.toml
```

Loading rules:

1. Built-in templates provide the base registry.
2. `main` must exist and is the fixed root-turn template.
3. `general` must exist and is the default delegated-task template.
4. Workspace templates use the file stem as `agent_id`.
5. Workspace templates override built-ins with the same `agent_id`.
6. The merged `HashMap<agent_id, AgentSpec>` is passed to orchd.

TOML schema:

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

---

## 4. Runtime Task DAG

Every task lifecycle event for an agent execution must carry both template and runtime identity:

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

The task DAG is the only source of truth for runtime agent relationships.

Root task:

```text
task_id = root task id (created at Session initialization or on the first turn, reused for subsequent root turns)
agent_id = "main"
parent_task_id = None
source_agent_id = None
```

Spawned task:

```text
task_id = generated child task id
agent_id = selected template id, or "general" if omitted
parent_task_id = parent runtime task id
source_agent_id = parent template id
```

---

## 5. Protocol DTO Rules

### `AgentSpec`

Represents a template.

Required identity fields:

```rust
AgentSpec {
    id,      // agent_id
    name,    // display name
    role,
    ...
}
```

### `AgentInfo`

Represents a runtime task instance projection for TUI.

Required identity fields:

```rust
AgentInfo {
    agent_id,        // template id
    task_id,         // runtime node id
    parent_task_id,  // runtime edge
    name,            // display snapshot from AgentSpec.name
    role,
    status,
}
```

`AgentInfo` is keyed by `task_id` in hostd and TUI state.

### Display / persist / approval / interaction events

Events tied to runtime work must carry both:

```text
task_id
agent_id
```

`task_id` routes to the runtime instance. `agent_id` groups by template and selects display labels/config context.

---

## 6. Storage Rules

Session storage is a directory:

```text
~/.piko/sessions/<encoded-cwd>/<session-id>/
  main.jsonl
  <agent-id>.jsonl
  tasks.json
```

Rules:

- `<agent-id>.jsonl` is a transcript shard grouped by template id.
- JSONL file name is not runtime identity.
- Message/tool entries written by an agent must include `agent_id` and `task_id`.
- `tasks.json` is the durable task DAG sidecar keyed by `task_id`.
- Resume reconstructs runtime relationships from `tasks.json`, not from JSONL file names.

---

## 7. TUI Rules

Agent panel rows represent runtime task instances.

Rules:

- Row key: `task_id`.
- Row label: `AgentSpec.name` or `agent_id`.
- Indentation/tree structure: `parent_task_id`.
- Status: task status.
- Duplicate labels are valid when the same template is spawned multiple times.

`AgentSpecList` and `AgentList` are different:

- `AgentSpecList` returns templates keyed by `agent_id`.
- `AgentList` returns runtime task instances keyed by `task_id`.

`AgentSubscribe { task_id }` returns the concrete view for one runtime task instance. `agent_id` remains metadata inside the returned snapshot and events.

---

## 8. Naming Rules

Use these names consistently:

| Use | Field |
|---|---|
| Template id | `agent_id` / `agentId` |
| Template display name | `name` inside `AgentSpec`, or `AgentInfo.name` as a display snapshot |
| Runtime task instance id | `task_id` / `taskId` |
| Runtime parent edge | `parent_task_id` / `parentTaskId` |
| Parent template metadata | `source_agent_id` / `sourceAgentId` |

Avoid:

- `agent_name` as an identity field.
- `parent_agent_id` for hierarchy.
- `agent_id` as a TUI row key.
- `name` as a storage or routing key.
- `subagent`, `generic`, or empty string as implicit default agent ids.
