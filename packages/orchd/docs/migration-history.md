## 19. Migration Plan

### Phase 1: Per-task Storage and User Message Symmetry

- protocol 新增 `PersistEvent::UserCommitted`。
- hostd persist consumer 支持 user message。
- orchd initial prompt 与 steer 使用同一 internal commit-user helper。
- local collector 收集 user persist event。
- 删除 `TurnSubmit` 直接 append user message。
- 删除 `{agent-id}.jsonl` / `main.jsonl` transcript 路由。
- 删除旧 `session.jsonl` 和独立 `tasks.json` 布局。
- root 和 child 全部直接写入 `tasks/{task-id}.jsonl`。
- session metadata 与 task index 合并写入 `session.json`。
- repository 只接受 `task_id` 路由 transcript。
- loader 只支持新的 per-task shard schema，不 dual-read、不 dual-write。
- resume 从 JSONL `Message::User` 恢复 transcript。
- 所有 entry 强制携带 `task_id + agent_id`。

### Phase 2: Unified Input API

- 新增 `SubmitTaskInput`、`InputReceipt`、`InputSource`。
- 将 `TaskSteerMessage` 替换为 mailbox `Input`。
- task creation 与 initial input 分离。
- hostd root、spawn tool、steer tool 和 queue 全部调用统一 API。
- 加入 request/message ID 幂等。

### Phase 3: Durable Barrier and Ordering

- 引入 `PersistSink` 与 `PersistAck`。
- user input 等待 durable ack。
- 加入 `task_seq`。
- 引入 per-task head，移除 transcript 对 session-global leaf 的依赖。

### Phase 4: orchd Structural Refactor

- 建立 `api/` facade，先代理现有 Supervisor。
- 将 `SessionSubscription/SessionOutputStream` 提升为正式 Agent API output contract。
- 以 session-scoped `SessionOutputHub` 替换 turn-scoped `SessionChannels/DispatchSenders`。
- public output 合并为 `SessionOutput::{Event, Delta}`，内部保留 reliable event 与 realtime delta 两个 QoS lane。
- 删除 public persist/lifecycle stream；persist 使用 request/ack，supervisor 使用 internal lifecycle observer。
- 建立 `runtime/task/input.rs`，收拢 input commit。
- 建立统一 TaskEventEmitter/sinks。
- 将 `runtime/orchestrator` 移为 `runtime/task`。
- 将 `dispatch` 拆为 `step/events/tools`。
- 将 Supervisor 拆为 service 与 supervision。
- 收紧 `lib.rs` public surface。

### Phase 5: Recovery Projection Cleanup

- transcript recovery 与 display replay 分离。
- repository 返回 task-oriented recovery model。
- agent view 从 committed task messages 独立投影。
- manifest task metadata 中的 `prompt` 明确降级为冗余审计元数据。

### Phase 6: Turn and Work Separation

- 保留 Turn 作为 hostd/session-level 用户交互概念。
- 引入 Work 作为 orchd/task-level execution cycle。
- 使用 `source_turn_id: Option<TurnId>` 显式关联 Work 与来源 Turn。
- 引入独立 Work lifecycle。
- task 长生命周期状态与单次 work result 解耦。
- Turn completion 不再承担 child/detached Work 的终止语义。

目录移动和持久化语义变更应拆成不同提交，避免同时进行大规模机械移动与行为修改。

---

