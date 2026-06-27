# 迁移任务状态：以 HostEvent 为线索

> 目标架构：`TUI (host-tui, TS) ↔ hostd (Rust) ↔ orchd (Rust)`  
> 原则：统一 HostEvent 建模，orchd 直接产出 HostEvent，删除中间事件层

---

## 1. 架构总览

```
                    ┌─────────────────────────────────────────────┐
                    │              新路径（主流）                   │
                    │                                             │
TUI (TS host-tui)   │   JSON-lines    hostd (Rust)   直接 link   orchd (Rust)
     │              │   HostEvent      server.rs     ─────────→  agent loop
     │              │   29 variants    turn_runner   订阅事件      tool registry
     │              │                  state                      step runner
     │              └─────────────────────────────────────────────┘
     │
     │              ┌─────────────────────────────────────────────┐
     │              │           旧路径（废弃中）                    │
     │              │                                             │
     └──────────────┤   host-runtime (TS)  ←RPC→  orchd (Rust)   │
                    │   run/controller.ts         orchestrator     │
                    │   event-bus.ts              OrchWireEvent    │
                    └─────────────────────────────────────────────┘
```

**HostEvent 定义位置**：

| 位置 | 语言 | 角色 |
|------|------|------|
| `orchd/src/protocol/host_event.rs` | Rust | **权威定义**（single source of truth） |
| `hostd/src/api/mod.rs` | Rust | `pub use orchd::protocol::host_event::*` re-export |
| `host-tui/src/client/hostd-protocol.ts` | TypeScript | 镜像（手工维护，待自动生成） |
| `host-runtime/src/orchd/protocol/host-event.ts` | TypeScript | 旧 EventBus 镜像（随 host-runtime 移除） |

---

## 2. 事件总览：29 个变体

```
Domain events (21) — 持久化到 JSONL journal           Streaming events (8) — 实时推送，不入库
─────────────────────────────────────────────────     ─────────────────────────────────
 1. user_message_submitted       ✅ emitted             22. message_start          ✅ emitted
 2. assistant_message_completed  ✅ emitted             23. message_end            ✅ emitted
 3. tool_result_committed        ✅ emitted             24. text_delta             ✅ emitted
 4. turn_started                 ✅ emitted             25. thinking_delta         ✅ emitted
 5. turn_completed               ✅ emitted             26. tool_start             ✅ emitted
 6. turn_failed                  ✅ emitted             27. tool_end               ✅ emitted
 7. turn_cancelled               ✅ emitted             28. approval_requested     ✅ emitted
 8. task_created                 ✅ emitted             29. approval_resolved      ❌ 缺口 #G2
 9. task_started                 ✅ emitted
10. task_completed               ✅ emitted
11. task_failed                  ✅ emitted
12. task_cancelled               ✅ emitted
13. task_transcript_committed    ✅ emitted
14. task_joined                  ✅ emitted
15. task_steered                 ❌ 缺口 #G1
16. session_created              ✅ emitted
17. session_opened *             ✅ emitted
18. session_listed *             ✅ emitted
19. state_snapshot *             ✅ emitted
20. queue_update                 ❌ 缺口 #G3
21. model_config_changed         ❌ 缺口 #G4
```

> \* `session_opened`、`session_listed`、`state_snapshot` 是设计文档发布后新增的事件，不在原始 `host-event-design.md` 中。

---

## 3. 逐事件追踪

### 3.1 完整链路（✅ 已完成）

以下事件从产生到 TUI 消费全链路打通：

| 事件 | Rust 产出位置 | TUI 映射 |
|------|--------------|---------|
| `turn_started` | `hostd/turn_runner.rs:197` — OrchTurnRunner | → `stream_started` |
| `turn_completed` | `hostd/turn_runner.rs:217` — OrchTurnRunner | → `stream_settled` |
| `turn_failed` | `hostd/state.rs` — fail_turn() | → `stream_settled` + `turn_failed` |
| `turn_cancelled` | `hostd/state.rs` — cancel_turn() | → `stream_settled` |
| `task_started` | `orchd/actors/agent/actor.rs:96` — handle_dispatch | → `task_started` |
| `task_completed` | `orchd/actors/agent/actor.rs:200` — finalize(Completed) | → `task_completed` |
| `task_transcript_committed` | `orchd/actors/agent/actor.rs:274` — emit_commit_events | → `task_transcript_committed` |
| `text_delta` | `orchd/actors/agent/step_runner.rs:79` — MessageDelta | → `assistant_delta` |
| `thinking_delta` | `orchd/actors/agent/step_runner.rs:88` — ThinkingDelta | → `thinking_delta` |
| `tool_start` | `orchd/tools/registry.rs:345` — execute_tool | → `tool_call_started` |
| `tool_end` | `orchd/tools/registry.rs:378` — emit_tool_finished | → `tool_call_ended` |
| `approval_requested` | `orchd/tools/registry.rs:420` — approval check | → `approval_needed` |
| `session_created` | `hostd/server.rs` — SessionCreate / fork / import | → `session_info_updated` |
| `session_opened` | `hostd/server.rs` — SessionOpen / fork / import / rename | → `session_info_updated` |
| `state_snapshot` | `hostd/server.rs` — SessionDelete / NavigateTo | → `session_info_updated` |
| `user_message_submitted` | `hostd/server.rs:451` — apply_turn_submit | → null（transcript reducer 消费） |
| `assistant_message_completed` | `orchd/actors/agent/actor.rs:307` — commit_events_from_result | → null（transcript reducer 消费） |
| `tool_result_committed` | `orchd/actors/agent/actor.rs:328` — commit_events_from_result | → null（transcript reducer 消费） |
| `task_created` | `orchd/orchestrator/task.rs:62` — spawn/spawn_detached/run | → null（未接入 TUI reducer） |
| `task_failed` | `orchd/actors/agent/actor.rs:230` — finalize(Failed) | → null（未接入 TUI reducer） |
| `task_cancelled` | `orchd/actors/agent/actor.rs:240` — finalize(Cancelled) | → null（未接入 TUI reducer） |
| `task_joined` | `orchd/orchestrator/task.rs:149` — await_task | → null（未接入 TUI reducer） |

### 3.2 TUI adapter 中明确返回 null 的事件

```
hostd-events.ts  switch 分支：

session_listed          → null    （会话列表事件，当前仅用于 hostd 内部）
user_message_submitted   → null    （由 transcript reducer 消费）
assistant_message_completed → null （由 transcript reducer 消费）
tool_result_committed    → null    （由 transcript reducer 消费）
message_start            → null    （可选边界事件）
message_end              → null    （可选边界事件）
default (未匹配的事件)    → null    （task_created, task_failed, task_cancelled, task_joined,
                                    task_steered, model_config_changed 落入此处）
```

### 3.3 四个缺口（❌ 待实现）

| ID | 事件 | 问题 | 影响 |
|----|------|------|------|
| **#G1** | `task_steered` | 枚举已定义，`is_domain` 已注册，但**无任何 Rust 代码 emit**。steer_task tool 未实现，agent loop 无 steering queue。 | 监督模型（parent → sub task 的 steering）完全不可用 |
| **#G2** | `approval_resolved` | 枚举已定义，TUI adapter 已处理。但 Rust tool_registry.rs:478 **emit 被注释掉**（注释写 "emit here only after approval state becomes host-visible"）。gateway 内部能处理 accept/decline，但 resolved 事件不回传 host。 | TUI 的 approval 弹窗无法收到 resolved 通知；用户确认/拒绝后工具继续执行但 TUI 不知道结果 |
| **#G3** | `queue_update` | 枚举已定义，`is_domain` 已注册，但**无任何 Rust 代码 emit**。hostd 没有实现 queue 管理（steer/follow-up/next-turn 计数 + preview）。 | TUI 无法展示队列状态；用户 steer 和 follow-up 功能不可用 |
| **#G4** | `model_config_changed` | 枚举已定义，`is_domain` 已注册，但 hostd `config_set` handler **只重建 TurnRunner，不 emit** 此事件。 | TUI 无法收到模型配置变更通知；依赖此事件刷新 UI 的功能失效 |

---

## 4. 旧路径（host-runtime TS）仍存留

`host-runtime` 包同时维护了独立的 EventBus + HostRunController，通过 RPC 连接 orchd。

### 4.1 旧路径的事件产出

`packages/host-runtime/src/host/run/controller.ts` 的 `runCore()` 方法：

1. 直接 publish 统一 HostEvent 到 EventBus：`turn_started`, `task_created`, `task_started`, `user_message_submitted`
2. 通过 `publishToEventBus()` 将 orchd 的 `OrchWireEvent` 映射为 HostEvent：
   - `message_start` / `message_end`
   - `text_delta` / `thinking_delta` → `text_delta` / `thinking_delta`
   - `tool_start` / `tool_end`
   - `approval_needed` → `approval_requested`
   - `approval_resolved` → `approval_resolved` ⚠️ 旧路径此处反而已实现！
   - `task_completed` / `task_failed` / `task_created` / `task_started` / `task_transcript_committed`
3. 最后 publish `turn_completed`

### 4.2 旧路径与新路径对比

| 功能 | 新路径 (hostd Rust) | 旧路径 (host-runtime TS) |
|------|---------------------|-------------------------|
| 审批 resolved | ❌ #G2（emit 注释掉） | ✅ publishToEventBus 已映射 |
| 队列管理 | ❌ #G3 | ✅ HostQueueController.steer() |
| 模型配置变更通知 | ❌ #G4 | ✅ config provider 更新可触发 |

### 4.3 旧路径待删除清单

| 删除项 | 阻塞条件 |
|--------|---------|
| `host-runtime/src/host/run/controller.ts` (HostRunController) | hostd OrchTurnRunner 功能完整 |
| `host-runtime/src/orchd/event-bus.ts` (EventBus) | 不再需要 TS 侧事件总线 |
| `host-runtime/src/orchd/protocol/` 中的 HostEvent TS 镜像 | 只保留 host-tui 侧镜像 |
| `host-runtime/src/orchd/rpc-client.ts` (OrchdRpcClient) | hostd 直接 link orchd |
| `host-runtime/` 整个包 | 所有功能迁移到 hostd |

---

## 5. Rust 侧事件产出位置完整索引

### 5.1 orchd 内部产出

| 文件 | 产出的 HostEvent | 触发时机 |
|------|-----------------|---------|
| `actors/agent/step_runner.rs` | `message_start`, `message_end`, `text_delta`, `thinking_delta` | 模型流式输出 |
| `tools/registry.rs` | `tool_start`, `tool_end`, `approval_requested` | 工具执行 + 审批检查 |
| `actors/agent/actor.rs` (`handle_dispatch`) | `task_started` | agent 接收 Dispatch 消息 |
| `actors/agent/actor.rs` (`finalize`) | `task_completed`, `task_failed`, `task_cancelled` | task 结束 |
| `actors/agent/actor.rs` (`emit_commit_events`) | `assistant_message_completed`, `tool_result_committed`, `task_transcript_committed` | task 完成时批量 commit |
| `orchestrator/task.rs` (`spawn`/`spawn_detached`/`run`) | `task_created` | 创建新 task |
| `orchestrator/task.rs` (`await_task`) | `task_joined` | detached task join |

### 5.2 hostd 产出

| 文件 | 产出的 HostEvent | 触发时机 |
|------|-----------------|---------|
| `server.rs` (`apply_turn_submit`) | `user_message_submitted` | 用户提交 turn |
| `server.rs` (SessionCreate / fork / import) | `session_created` | 会话创建 |
| `server.rs` (SessionOpen / fork / import / rename) | `session_opened` | 会话打开或信息变更 |
| `server.rs` (SessionList) | `session_listed` | 列出所有会话 |
| `server.rs` (SessionDelete / NavigateTo) | `state_snapshot` | 状态变更后快照 |
| `turn_runner.rs` (OrchTurnRunner) | `turn_started`, `turn_completed` | turn 生命周期 |
| `state.rs` | `turn_failed`, `turn_cancelled` | 错误/取消 |

---

## 6. 剩余功能缺口（非事件维度）

除了四个事件缺口，以下 hostd 功能也需要补齐：

| 功能 | 状态 | 复杂度 | 对应事件 |
|------|------|--------|---------|
| **队列管理** (steer/follow-up/next-turn) | 未实现 | 中 | `queue_update` + `task_steered` |
| **Compaction 执行** (LLM 摘要 + 自动触发) | 仅骨架类型 | 中 | — |
| **Multi-agent** (spawn sub-task) | orchd 已支持 spawn/spawn_detached/await_task，hostd 调用的 run() 函数只创建根 task | 高 | `task_created`, `task_joined`, `task_steered` |
| **审批 resolved 回传** | orchd gateway 内部处理但事件不回传 | 低 | `approval_resolved` (#G2) |
| **模型配置变更通知** | config_set 重建 runner 但不 emit | 低 | `model_config_changed` (#G4) |
| **OAuth device code flow** | 未实现 | 中 | — |
| **MCP 工具集成** | 未实现 | 高 | — |
| **运行时工具切换** | active tools 未接入 hostd 命令 | 低 | — |

---

## 7. 事件流完整示例（当前 Rust 路径）

### 7.1 单 agent Turn（当前实现状态）

```
TUI → hostd: {"type":"turn_submit","session_id":"s1","text":"写一个函数"}
hostd → TUI: {"type":"command_accepted","command_id":"cmd1"}           ← CommandAck

hostd → TUI: {"type":"user_message_submitted",...}                    ✅
hostd → TUI: {"type":"turn_started",...,"root_task_id":"t1"}          ✅
hostd → orchd: OrchCore::run(prompt, host_context)

  orchd → hostd → TUI: {"type":"task_created","agent_id":"main",...}  ✅
  orchd → hostd → TUI: {"type":"task_started","agent_id":"main",...}  ✅

  ── Step 1 ──
  orchd → hostd → TUI: {"type":"message_start",...}                   ✅
  orchd → hostd → TUI: {"type":"text_delta","delta":"Let"}            ✅
  orchd → hostd → TUI: {"type":"text_delta","delta":" me"}            ✅
  orchd → hostd → TUI: {"type":"tool_start","tool_name":"read",...}   ✅
  orchd → hostd → TUI: {"type":"message_end",...}                     ✅
  orchd → hostd → TUI: {"type":"tool_end","tool_name":"read",...}     ✅

  ── Step 2 ──
  orchd → hostd → TUI: {"type":"message_start",...}                   ✅
  orchd → hostd → TUI: {"type":"text_delta","delta":"Done!"}          ✅
  orchd → hostd → TUI: {"type":"message_end",...}                     ✅

  orchd → hostd → TUI:
    {"type":"assistant_message_completed",...}                         ✅
    {"type":"tool_result_committed",...}                               ✅
    {"type":"task_transcript_committed",...}                           ✅
  orchd → hostd → TUI: {"type":"task_completed","final_status":"completed"} ✅

hostd → TUI: {"type":"turn_completed","total_tasks":1}                ✅
```

### 7.2 审批流程（当前状态 — #G2 缺口）

```
  orchd → hostd → TUI: {"type":"tool_start","tool_name":"bash","args":"rm -rf /"}   ✅
  orchd → hostd → TUI: {"type":"approval_requested","approval_id":"apr1",...}        ✅
      │
      │  ← 用户确认 (TUI → hostd: approval.respond — 命令已定义，gateway 可处理)
      │
  orchd → hostd → TUI: {"type":"approval_resolved",...}              ❌ #G2 不产出
  orchd → hostd → TUI: {"type":"tool_end","result":"...",...}                        ✅
```

---

## 8. 建议的实现顺序

按依赖关系和影响面排序：

| 优先级 | 任务 | 预计工作量 | 解锁能力 |
|--------|------|-----------|---------|
| **P0** | #G2 `approval_resolved` — 在 tool_registry 中恢复 emit | 小（取消注释 + 补 approval_id 字段） | 审批流程完整闭环 |
| **P0** | #G4 `model_config_changed` — 在 hostd config_set handler 中 emit | 小（一行 emit） | TUI 模型配置 UI 正常工作 |
| **P1** | #G3 `queue_update` + steering queue — 在 hostd 中实现队列管理 | 中 | 用户 steer / follow-up 功能 |
| **P1** | #G1 `task_steered` — 实现 steer_task tool + agent loop steering queue | 中 | 监督模型可用 |
| **P2** | Compaction LLM 摘要执行 | 中 | 长对话 token 管理 |
| **P2** | Multi-agent turn 路径（hostd 侧调用 spawn/spawn_detached） | 高 | 多 agent 协同 |
| **P3** | 删除 host-runtime TS 包 | — | 架构收敛完成 |
| **P3** | OAuth / MCP / 工具切换 | 中-高 | 完整功能对等 |
