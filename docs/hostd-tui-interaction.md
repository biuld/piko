# Hostd ↔ TUI Interaction

hostd 和 TUI 通过 JSON-lines stdio 双向通信。hostd 是 TUI 可见状态的权威来源；TUI 只发送 commands，并消费 hostd 输出的 `ServerMessage`。

本文档是线协议和状态所有权规范。

---

## 1. Ownership

| State | Owner | Notes |
|---|---|---|
| Session JSONL | hostd | 由 `PersistEvent` 和 host commands 写入 |
| Transcript | hostd | 从 JSONL/snapshot 恢复，发给 orchd 作为 history |
| Task DAG | hostd | 从 `TaskEvent` 维护，供 agent tree 和 resume 使用 |
| Turn state | hostd | turn start/completion/cancel/failure |
| Approval state | hostd | pending approval、response、scope grant |
| User interaction state | hostd | pending questions、response、timeout/cancel |
| Queue | hostd | steer/follow-up/next-turn queues |
| Config/auth/model state | hostd | TUI 通过 command 修改或查询 |
| Per-agent live views | hostd | materialized timeline + replay cursor per task/agent |
| Foreground rendering | TUI | 从 hostd agent view snapshot 和增量事件渲染 |

TUI 不直接消费 orchd。orchd 事件必须先进入 hostd event bus。

---

## 2. Wire Protocol

```text
TUI ── JSONL stdin  ──> hostd   Command
TUI <─ JSONL stdout ── hostd   ServerMessage
```

每行一个 JSON object。每个 command 带 `command_id`，hostd 对 command 返回 `CommandResponse`。

---

## 3. Commands

```rust
pub enum Command {
    SessionCreate { command_id, cwd },
    SessionOpen { command_id, session_id, session_path? },
    SessionList { command_id, scope, cwd? },
    SessionNavigate { command_id, session_id, entry_id, summarize, custom_instructions? },
    SessionFork { command_id, session_id, entry_id? },
    SessionImport { command_id, path },
    SessionRename { command_id, session_id, name },
    SessionDelete { command_id, session_id },
    SessionSetLabel { command_id, session_id, entry_id, label? },

    TurnSubmit { command_id, session_id, text },
    TurnCancel { command_id, session_id, turn_id },

    QueueSteer { command_id, session_id, task_id, message },
    QueueFollowUp { command_id, session_id, message },
    QueueNextTurn { command_id, session_id, message },

    ApprovalRespond { command_id, session_id, approval_id, decision, note? },
    UserInteractionRespond { command_id, session_id, interaction_id, response },

    StateSnapshot { command_id, session_id },
    EventsResume { command_id, session_id, after_seq },

    ModelList { command_id },
    CommandCatalogGet { command_id },
    ConfigGet { command_id, namespace },
    ConfigUpdate { command_id, patch },
    SessionCompact { command_id, session_id },

    AuthSetApiKey { command_id, provider, api_key },
    AuthLoginOAuth { command_id, provider },
    AuthLogout { command_id, provider },

    AgentSpecList { command_id },
    AgentList { command_id, session_id },
    AgentSubscribe { command_id, session_id, agent_id },
    AgentUnsubscribe { command_id, session_id, agent_id },
}
```

---

## 4. Server Messages

```rust
pub enum ServerMessage {
    CommandResponse { command_id, result },
    Auth(AuthEvent),
    Display(DisplayEvent),
    TaskLifecycle(TaskEvent),
    TurnLifecycle(TurnEvent),
    Approval(ApprovalEvent),
    Queue(QueueEvent),
    Model(ModelEvent),
    AgentConnected { agent_id, parent_task_id?, name, role },
    AgentDisconnected { agent_id, task_id, reason },
}
```

Message categories:

| Category | Message | Source | TUI consumer |
|---|---|---|---|
| Command result | `CommandResponse` | hostd command handler | effect resolver |
| Live rendering | `Display` | orchd via hostd | timeline |
| Task state | `TaskLifecycle` | orchd via hostd | status, agent tree |
| Turn state | `TurnLifecycle` | hostd | editor, bottom bar, status |
| Approval | `Approval` | hostd approval state | approval panel |
| Queue | `Queue` | hostd queue state | bottom bar |
| Model | `Model` | hostd model config | model selector |
| Auth | `Auth` | hostd auth flow | status/timeline |
| Agent membership | `AgentConnected/Disconnected` | hostd task DAG projection | agent panel |

---

## 5. Hostd Event Bus

hostd merges four classes of inputs:

```text
orchd persist channel    -> hostd storage/state
orchd display channel    -> ServerMessage::Display
orchd lifecycle channel  -> hostd task state + TaskLifecycle/TurnLifecycle
host-managed side events -> Approval/Queue/Model/Auth/CommandResponse
```

Rules:

- persist input is consumed before a turn is considered complete.
- display input is applied to the per-agent live view store and then forwarded to the active TUI subscription.
- lifecycle input updates task DAG and agent projections.
- approval and user interaction are hostd-managed workflows, even when initiated by an orchd tool.
- hostd never forwards persist events directly to TUI; TUI receives snapshots and display events.

---

## 6. Turn Flow

```text
TUI:   TurnSubmit { session_id, text }
hostd: append user Message to JSONL/state
hostd: start turn and call orchd
hostd -> TUI: TurnLifecycle::Started

orchd -> hostd: TaskEvent::Created(root task_id)
hostd -> TUI: AgentConnected(main/root task)
hostd -> TUI: TaskLifecycle::Created

orchd -> hostd: DisplayEvent::MessageStart
orchd -> hostd: DisplayEvent::TextDelta*
orchd -> hostd: DisplayEvent::ThinkingDelta*
orchd -> hostd: DisplayEvent::MessageEnd
orchd -> hostd: PersistEvent::Finalized
orchd -> hostd: DisplayEvent::Finalized

orchd -> hostd: PersistEvent::ToolCallCommitted*
orchd -> hostd: DisplayEvent::ToolStarted*
orchd -> hostd: DisplayEvent::ToolEnded*
orchd -> hostd: PersistEvent::ToolResultCommitted*

orchd -> hostd: TaskEvent::Completed | Failed | Cancelled
hostd -> TUI: TaskLifecycle terminal event
hostd -> TUI: AgentDisconnected
hostd -> TUI: TurnLifecycle::Completed | Failed | Cancelled
```

Root task id is created by orchd and introduced through `TaskEvent::Created`. hostd does not create a competing root task id.

---

## 7. Approval Flow

```text
tool provider -> hostd approval gateway
hostd: create pending approval state
hostd -> TUI: Approval::Requested
TUI -> hostd: ApprovalRespond
hostd: resolve pending approval and persist wider-scope grant if needed
hostd -> TUI: Approval::Resolved
hostd -> tool provider: decision
```

Rules:

- pending approvals are hostd state and appear in snapshots.
- approval response must be routed by `approval_id`.
- if hostd/TUI disconnects, pending approval resolves by explicit cancellation/decline semantics.

---

## 8. User Interaction Flow

```text
tool provider -> hostd user interaction gateway
hostd: create pending interaction state
hostd -> TUI: DisplayEvent::InteractionRequested
TUI -> hostd: UserInteractionRespond
hostd -> TUI: DisplayEvent::InteractionResolved
hostd -> tool provider: response
```

Rules:

- pending interactions are hostd state and appear in snapshots.
- interaction display events are live rendering events; hostd state is the recovery source.
- auto-resolution is evaluated by hostd, not TUI alone.

---

## 9. Agent List and Switching

`AgentList` returns a projection of the task DAG for the current session. `AgentSubscribe` selects which agent timeline the TUI wants to foreground and returns a hostd-owned agent view.

### Hostd agent view store

hostd maintains a per-session, per-task live view store:

- `task_id`
- `agent_id`
- `parent_task_id`
- materialized timeline entries
- pending interactions/approvals relevant to the view
- terminal task status and summary
- sequenced replay log cursor

Display events are applied to this store before hostd forwards them. Inactive agents keep receiving updates in the store even when TUI is foregrounded on another agent.

### AgentSubscribe response

`AgentSubscribe { session_id, agent_id, after_seq? }` returns:

- `AgentSubscribed { agent_id, snapshot, next_seq }`
- `snapshot.task_views[]` for each task executed by that agent
- replay events newer than `after_seq`
- live incremental events for that foreground subscription

Switching constraints:

- Switching does not pause orchd execution.
- hostd must not discard or ignore events for inactive agents.
- A subscribed agent view is reconstructed from hostd live view store.
- completed agents remain visible until the session is closed or explicitly pruned.

`AgentConnected` and `AgentDisconnected` are projections of task lifecycle, not separate sources of truth.

---

## 10. Session Persistence and Resume

Storage path:

```text
~/.piko/sessions/<encoded-cwd>/<session-id>.jsonl
```

Resume flow:

```text
TUI -> hostd: SessionOpen
hostd: read JSONL + task/approval/interaction metadata
hostd -> TUI: CommandResponse::SessionOpened { snapshot }
TUI: rebuild timeline, tree, queue, pending workflows from snapshot
```

Snapshot contains the state needed to reconstruct UI without replaying display deltas.

Display deltas are not stored as transcript. They are live-only.
