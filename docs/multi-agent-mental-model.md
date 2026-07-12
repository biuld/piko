# Multi-Agent Mental Model

本文档定义 piko 多 agent 系统的稳定心智模型，重点回答 4 个问题：

1. `agent_id`、`task_id`、`turn_id` 分别代表什么。
2. 多 agent 场景里谁是长生存实体，谁是一次性交互。
3. 父子 task、spawn、steer、poll 在运行时如何关联。
4. orchd 的 stream runtime 应该消费哪些输入，并把结果分发给哪些下游 consumer。

运行时事件边界见 `packages/orchd/docs/events-and-observation.md` 与 `packages/orchd/docs/task-runtime.md`；模板与持久化规范见 `docs/agent-architecture.md`。

---

## 1. 三层对象模型

多 agent 系统围绕 3 个不同层次的对象运转：

```text
Agent Spec (template)
  └─ Agent Task (runtime instance)
       └─ Turn / Work Cycle
```

### Agent Spec

- 静态模板，主键是 `agent_id`。
- 定义 system prompt、模型、思考等级、工具集和展示元数据。
- 只描述能力，不持有运行时状态。

### Agent Task

- 运行时实例，主键是 `task_id`。
- 一个 task 是一个长生存的 agent 执行体。
- task 持有自己的 transcript、挂起状态、子任务关系和未完成工作。
- 同一个 `agent_id` 可以在同一 session 中被实例化多次，每次都对应不同 `task_id`。

### Turn / Work Cycle

- 单次输入到输出的执行周期，不是运行时节点主键。
- 一个 task 可以跨越多个 turn/work cycle。
- turn 是短生存的；task 是长生存的。

结论：

- `agent_id` 是模板身份。
- `task_id` 是运行时身份。
- `turn_id` 是会话回合身份。
- 多 agent DAG 的节点主键永远是 `task_id`，不是 `agent_id`。

---

## 2. 长生存 Task 模型

### Root Task

- 一个 session 只有一个根 task。
- 根 task 的模板固定为 `agent_id = "main"`。
- 用户后续的所有输入都 steer 到这个同一个 root task，而不是每轮重建新 root task。
- supervisor 查找可复用 root 时必须同时匹配 `session_id` 和 `agent_id`，不能跨 session 复用同一 handle。

### Spawned Task

- `spawn` / `spawn_detached` 创建新的 child task。
- child task 拥有独立 transcript、独立执行循环、独立生命周期。
- child task 完成当前工作后回到 `Idle`，而不是立即销毁。
- 后续父 task 或用户可以再次 `steer_task` 到同一个 child task，复用上下文和记忆。

### 工作结束与关闭语义

- supervisor 启动的长生存 task 在正常工作结束后进入 `Idle`，继续接受 steer。
- `Failed` 描述最近一次工作失败；runtime handle 保留，后续 steer 可再次进入 `Running`。
- `Closed` 表示暂停接收 steer，可通过 `Reopen` 回到 `Idle`。
- `Completed` 用于显式 one-shot runtime；`Cancelled` 表示 cancellation token 已终止该 runtime，二者会回收 active handle。

---

## 3. 多 Agent DAG

运行时层级由 task DAG 表达：

```text
task_main (agent_id=main)
├─ task_scout_1 (agent_id=scout)
├─ task_coder_1 (agent_id=coder)
│  └─ task_reviewer_1 (agent_id=reviewer)
└─ task_scout_2 (agent_id=scout)
```

约束：

- DAG 边由 `parent_task_id` 定义。
- `source_agent_id` 记录是谁发起了 spawn，但不替代 `parent_task_id`。
- 同一个 `agent_id` 可以在树上出现多次，彼此独立。
- TUI 和 hostd 订阅、恢复、切换视图时都以 `task_id` 为主键。

---

## 4. 多 Agent 场景中的 4 类操作

### `spawn`

- 父 task 发起委派。
- 父 task 只发起 spawn 请求，不直接拥有 child task 的创建权。
- orchd 的 supervisor 负责分配 `task_id`、注册 child task handle、启动 child task runtime，并等待 child task 产出首个工作报告。
- child report 作为父 task 当前 tool call 的结果返回。
- child task 返回报告后转入 `Idle`，保持可继续 steer。

### `spawn_detached`

- 创建 child task 后立即返回 `{ task_id, status: "detached" }`。
- 父 task 不等待 child 完成。
- child task 由 supervisor 保持在全局注册表中，继续独立运行，事件照常流向 hostd。

### `poll_task`

- 查询一个已知 `task_id` 最近一次完成的工作报告。
- 查询的是 supervisor 维护的 task result 缓存，不是直接读取某个父 task 的局部状态。
- 不改变目标 task 的执行状态。
- 用于 detached 场景下的异步收集。

### `steer_task`

- 向一个已存在且未关闭的 task 注入新的用户/父任务指令。
- 注入目标由 supervisor 的全局 task handle 注册表解析。
- 目标 task 下一次被调度时继续从已有 transcript 往后执行。
- steer 不应创建新 task，也不应清空旧 transcript。

---

## 5. Supervisor 是独立运行时实体

除了 agent spec、task、turn 之外，多 agent 系统里还存在一个关键运行时实体：`Supervisor`。

它不是用户可见的 task，也不是 agent 模板，而是 orchd 的全局 task registry 和控制平面。

### Supervisor 的职责

- 为新 task 分配运行时 `task_id`。
- 维护全局 task handle 注册表。
- 启动和持有每个 task 的 driver / stream。
- 维护 task result 缓存，供 `spawn` 和 `poll_task` 读取。
- 接收 `steer_task`、`close_task`、取消等跨 task 控制请求。
- 在 task 退出或关闭后回收 handles 和注册项。

### Supervisor 不负责什么

- 不拥有 task transcript。
- 不直接执行 LLM step。
- 不直接渲染用户视图。
- 不替代 hostd 成为持久化权威层。

### 关系模型

```text
Agent Spec Registry (hostd-owned template registry)
        │
        ▼
Supervisor (orchd-owned runtime registry)
        ├─ task_main handle
        ├─ task_scout_1 handle
        ├─ task_coder_1 handle
        └─ task_reviewer_1 handle
```

关键边界：

- `spawn` 是 task 对 supervisor 的请求，不是 task 自己在本地“new 一个 child loop”。
- task DAG 的运行时实例化入口在 supervisor。
- 任何跨 task 操作都应经过 supervisor 的全局注册表，而不是父 task 直接持有子 task 指针。

---

## 6. Runtime 中真正存在的输入流

多 agent runtime 不只有 LLM stream。以一个 task 的执行器视角，稳定的输入流至少有 6 类：

| 输入流 | 来源 | 典型载荷 | 作用对象 |
|---|---|---|---|
| `TaskStart` | hostd / supervisor | 初始 prompt、host context、senders | 启动一个 task |
| `SteerInput` | 用户、父 task、hostd | message、source_task_id、source_agent_id、senders | 唤醒或继续一个 idle task |
| `GatewayEvent` | llmd | content delta、reasoning delta、tool call chunk、usage、done、error | 驱动单次 model step |
| `ToolExecutionResult` | regular tool runtime / spawner | result payload、error、spawn report | 回写 transcript 并决定是否继续下一 step |
| `Cancellation` | hostd / supervisor | cancel token、shutdown signal | 中止当前 step 或整个 task |
| `LifecycleControl` | hostd / supervisor | close、reopen、driver closed | 改变 task 可交互状态 |

结论：

- `GatewayEvent` 只是其中一种输入。
- `dispatch` 如果只围绕 gateway stream 建模，就一定会把 steer、cancel、spawn、close 这些职责泄漏回 `agent_loop`。

---

## 7. Downstream Consumer 面

对一个 task runtime 而言，输入被处理后会流向一组稳定的下游 consumer。它们消费的是已经按职责分流好的结果，而不是原始用户命令。

### 7.1 Transcript Consumer

- 维护 task 的内存 transcript。
- 合并 `UserMessage`、`AssistantMessage`、`ToolCallMessage`、`ToolResultMessage`。
- 只负责运行时上下文，不负责落盘。

### 7.2 Realtime Observation Consumer

- 产出 best-effort `SessionOutput::Delta`。
- 用于 TUI 流式渲染文本、thinking 和 tool call delta；tool lifecycle 与 interaction 使用各自的可靠领域事件。
- realtime delta 只影响即时可见性，不承担恢复语义。

### 7.3 Persist Consumer

- 产出 `PersistEvent`。
- 负责 assistant finalized、tool call committed、tool result committed、task lifecycle committed。
- 是 session 恢复和 transcript 重建的事实来源。

### 7.4 Lifecycle Consumer

- 产出 `LifecycleEvent`。
- 表达 `Created`、`Started`、`Steered`、`Idle`、`Completed`、`Failed`、`Cancelled`、`Closed` 等状态迁移。
- 不直接决定 transcript 内容，但决定 task DAG 和工作状态。

### 7.5 Tool Runtime Consumer

- 对完整 tool call 集合做 regular tool 执行或 spawn tool 调度。
- 对外发出 tool started/tool ended/tool result committed。
- 返回结构化 tool result 供 transcript 和后续 step 使用。

### 7.6 Host View Consumer

- 严格说它在 hostd，不在 orchd。
- hostd 消费 display/persist/lifecycle 后，物化为 per-task live view、session JSONL、task metadata、approval/queue state。

---

## 8. 多 Agent 下的事件传播模型

一个父 task spawn 子 task 后，系统中会同时存在两条独立但相关的运行线：

```text
Parent task runtime
  └─ tool call "spawn"
       └─ supervisor runtime registry
            └─ child task runtime
                 ├─ own gateway step
                 ├─ own tool execution
                 ├─ own lifecycle events
                 └─ own transcript
```

关键点：

- 父 task 和子 task 各自运行自己的 `agent_loop`。
- supervisor 是父子 task 之间的注册与控制中介。
- 子 task 的 display/persist/lifecycle 事件进入同一个 session event bus，但都带自己的 `task_id`。
- 父 task 是否等待子 task，只影响父 task 自己的控制流，不影响 child event 的产生方式。
- child task 进入 `Idle` 后仍然存在，后续可以再次被 steer。

---

## 9. 推荐的编排分层

为了避免 `agent_loop` 变成全能体，建议把 runtime 分成 3 层：

### 9.1 Turn / Task Orchestrator

- 接受 `TaskStart`、`SteerInput`、`Cancellation`、`LifecycleControl`。
- 决定 task 当前是启动、运行、挂起、关闭还是恢复。
- 维护 task 级状态机。

### 9.2 Step Dispatch

- 接受单次 model step 的 `GatewayEvent` 流。
- 聚合 assistant message、tool call chunk、usage、error。
- 产出 `StepDispatchResult`；不定义统一 effect union。

### 9.3 Effect Consumers / Sinks

- 消费 `StepDispatchResult`、`ToolExecutionResult` 和 task-level transition。
- 分别生成 transcript / display / persist / lifecycle / tool runtime 结果。

这样：

- `agent_loop` 是状态机。
- `dispatch` 是输入分流器。
- `consumer` 是专用结果对象的生产器或 sink。

---

## 10. 当前设计应满足的多 Agent 不变量

### 身份不变量

- `task_id` 是 runtime 主键。
- `agent_id` 不是 runtime 主键。
- `parent_task_id` 才定义 DAG。
- supervisor registry 是 task runtime 的全局索引，不是 DAG 节点。

### 生命周期不变量

- task 是长生存实体。
- 工作完成不等于 task 关闭。
- 未显式关闭的 task 可以反复 steer。

### 事件不变量

- 每个 task 都独立产出 display/persist/lifecycle 事件。
- 事件必须携带 `task_id` 和 `agent_id`。
- child task 的事件不能被父 task 吞掉或覆盖。

### 通道不变量

- `senders=None` 只应意味着“本地收集模式”，不应意味着事件静默丢弃。
- 本地测试与生产路径应共享同一批分流结果，只是落地点不同。

---

## 11. 文档化后的实现目标

这份心智模型对应的目标实现不是“一个巨大 agent loop”，而是：

1. orchd 有一个 task 级编排器，处理 start / steer / cancel / close。
2. orchd 有一个 supervisor 级注册表，负责 spawn、task registration、handle 管理、结果缓存和回收。
3. orchd 有一个 step 级 dispatch，处理 gateway stream。
4. tool execution 是标准 downstream consumer，而不是 loop 里的特殊分支。
5. lifecycle 是标准 downstream consumer，而不是 loop 里的硬编码 side effect。
6. hostd 继续做用户可见状态的权威层，但不替 orchd 发明新的 runtime node identity。
