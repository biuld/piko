## 3. Core Model

统一对象模型：

```text
AgentSpec
  static capability template, keyed by agent_id

AgentTask
  long-lived runtime instance, keyed by task_id

Turn
  session-level user interaction, keyed by turn_id

Work
  task-level input-driven execution cycle, keyed by work_id

Message
  durable transcript fact, keyed by message_id
```

### 3.1 Identity

| Identity | Meaning | Lifetime |
|---|---|---|
| `session_id` | hostd session | 整个会话 |
| `agent_id` | AgentSpec 模板 | 静态配置生命周期 |
| `task_id` | agent runtime instance | 跨多个 work |
| `turn_id` | hostd/session 用户交互 | 用户提交到该 session turn 完成 |
| `work_id` | 某个 task 的一次输入驱动执行周期 | input accepted 到 succeeded/failed/cancelled |
| `message_id` | transcript message | 永久稳定 |
| `request_id` | API 调用幂等键 | 至少覆盖重试窗口 |

`turn_id` 与 `work_id` 不是别名，也不互相替代。Turn 属于 hostd/session 交互模型；Work 属于 orchd/task runtime 模型。root task 的一次 work 通常由某个 Turn 触发，但一个 Turn 可以触发多个 task Work。

所有 runtime 定位必须使用 `task_id`。禁止使用 `agent_id` 执行 steer、cancel、resume 或 view routing。

### 3.2 Turn and Work

Turn 表达用户或 hostd 对 session 发起的一次交互：

```text
Turn
  user/session input
  → root task processing
  → user-visible turn completion
```

Work 表达一个具体 task 为处理一条 accepted input 而进行的一次执行：

```text
Work
  accepted task input
  → one or more model/tool steps
  → succeeded/failed/cancelled
```

基数关系：

```text
Session 1 ── N Turn
Session 1 ── N Task
Task    1 ── N Work
Turn    1 ── 1..N Work
Work    ── source_turn_id ──> 0..1 Turn
```

最后一条表示一个 Work 最多引用一个来源 Turn，但不要求所有 Work 都属于 Turn。detached continuation、父 task 在原 Turn 结束后的 steer，或系统触发的 task input 都可以产生 `source_turn_id = None` 的 Work。

```rust
pub struct WorkContext {
    pub work_id: WorkId,
    pub task_id: TaskId,
    pub source_turn_id: Option<TurnId>,
}
```

典型同步多 agent turn：

```text
turn_42
├─ task_main / work_main_7
├─ task_coder_1 / work_coder_3
└─ task_reviewer_1 / work_reviewer_2
```

三个 Work 都可以记录 `source_turn_id = Some(turn_42)`，但各自拥有独立的 `task_id/work_id`、状态、结果和 cancellation。

detached continuation：

```text
turn_42 completes

task_coder_1 / work_coder_4
  source_turn_id = None
```

因此：

- Turn completion 不意味着其触发的所有 detached task 都被 terminate。
- Work failure 不意味着 Task 永久失败；Task 可以接受下一条 input 创建新的 Work。
- `CancelWork` 只取消指定 task 的指定执行，不取消整个 Turn 或销毁 Task。
- Turn 用于 hostd/TUI 的用户交互状态；Work 用于 orchd 的执行、poll、queue、result 和 cancellation。

### 3.3 Ownership

| State | Owner |
|---|---|
| AgentSpec registry | hostd 配置权威；orchd 持有运行副本 |
| Active task handles | orchd supervisor |
| In-memory task transcript | task runtime |
| Durable transcript | hostd JSONL |
| Task DAG durable projection | hostd |
| Task DAG live registry | orchd supervisor |
| Session tree selection | hostd |
| TUI timeline/view | hostd projection + TUI local state |

关键边界：

- orchd 决定什么进入 transcript。
- hostd 决定 committed fact 是否已经 durable。
- supervisor 不拥有 transcript。
- lifecycle metadata 不能替代 transcript message。

---

