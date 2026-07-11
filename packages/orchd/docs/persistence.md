## 8. Persistence Barrier

将 persist event 放进 channel 不等于数据已经写入 JSONL。若要求 user message 在 LLM step 前 durable，需要 acknowledgement barrier。

orchd-owned port：

```rust
#[async_trait]
pub trait PersistSink: Send + Sync {
    async fn commit_message(
        &self,
        event: MessageCommit,
    ) -> Result<PersistAck, PersistError>;

    async fn commit_task_event(
        &self,
        event: TaskEvent,
    ) -> Result<PersistAck, PersistError>;

    async fn commit_work_event(
        &self,
        event: WorkEventCommit,
    ) -> Result<PersistAck, PersistError>;
}
```

```rust
pub struct PersistAck {
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub message_id: Option<MessageId>,
    pub task_seq: u64,
}
```

hostd 提供实现或 channel-backed bridge：

```text
orchd requests commit
  → hostd validates identity/order
  → hostd appends JSONL
  → hostd updates HostState projection
  → hostd returns PersistAck
  → orchd appends in-memory transcript
  → committed event becomes observable
  → orchd starts LLM step
```

`PersistSink` trait 不放 `piko-protocol`，因为 protocol crate 只承载 serializable DTO，不承载 runtime trait。

如果第一阶段不实现 ack，只能保证事件顺序：

```text
UserCommitted emitted before assistant/tool events
```

此时不能宣称“user durable before LLM”。文档和测试必须区分 emitted guarantee 与 durable guarantee。

---

## 9. Idempotency and Ordering

### 9.1 Idempotency

- `request_id` 是 API operation 幂等键。
- `message_id` 是 transcript message 幂等键。
- 相同 `request_id + task_id` 重试返回原 receipt。
- 相同 `message_id` 不重复 append。
- 同一 request ID 携带不同 payload 返回 `IdempotencyConflict`。

### 9.2 Per-task Sequence

每个 task durable fact 携带单调递增的 `task_seq`：

```text
TaskCreated                 seq 1
initial UserCommitted       seq 2
WorkStarted                 seq 3
AssistantCommitted          seq 4
ToolCallCommitted           seq 5
ToolResultCommitted         seq 6
WorkSucceeded/TaskIdle      seq 7
next UserCommitted          seq 8
```

`task_seq` 用于：

- 检测丢失与乱序；
- per-task replay；
- 幂等提交；
- agent view 增量订阅；
- 不依赖多个 channel 的到达顺序。

hostd 可以继续维护 session-global view sequence；它与 `task_seq` 是两个不同概念。

---

## 10. Storage and Recovery

### 10.1 Message Entry Requirements

所有新写入的 transcript entry 必须包含：

```rust
pub struct MessageEntry {
    pub id: MessageId,
    pub parent_id: Option<MessageId>,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub work_id: WorkId,
    pub task_seq: u64,
    pub timestamp: i64,
    pub message: Message,
}
```

存储 schema 直接切换，不提供旧 message entry 的兼容读取。所有 entry 必须包含完整的 `task_id + agent_id`；缺少 runtime identity 的 entry 视为无效数据并 fail closed。

### 10.2 Per-task Head

session tree selection 与 task transcript head 必须分离：

```text
session current_leaf_id
  用户在 session tree 中选中的节点

task_heads[task_id]
  该 task 最后一条 committed transcript message
```

每次 message commit：

```text
parent_message_id = task_heads[task_id]
append message
task_heads[task_id] = message_id
```

禁止继续使用 session-global `current_leaf_id` 作为所有 task message 的 parent，否则并发 task 会形成跨 task、跨 shard 链。

### 10.3 Per-task Shard Layout

存储粒度与 runtime identity 完全一致：一个 task 对应一个 JSONL shard，文件路由只使用 `task_id`，不得使用 `agent_id`。

目标布局：

```text
session/
├── session.json
└── tasks/
    ├── {task-id-1}.jsonl
    ├── {task-id-2}.jsonl
    └── {task-id-3}.jsonl
```

具体规则：

1. root task 也写入 `tasks/{root-task-id}.jsonl`，不再使用 `main.jsonl` 作为 transcript shard。
2. child task 写入 `tasks/{child-task-id}.jsonl`。
3. `agent_id` 保存在 task metadata 和每条 message entry 中，只表示该 task 使用的 AgentSpec。
4. 同一 `agent_id` 的多个 task 必须写入不同文件。
5. 一个 task 的 user、assistant、tool call 和 tool result 全部写入同一个 shard。
6. `parent_message_id` 只能引用当前 shard 内同一 task 的 message。
7. repository API 只接受 `task_id` 路由 transcript，不接受 `agent_id` 作为文件定位参数。
8. task shard 保存该 task 的 message 与 lifecycle durable facts，是 task 恢复的权威来源。
9. `session.json` 是 session manifest：保存 session metadata、root/active task pointer，以及可从 task shards 重建的 task DAG/status 索引。
10. shard 路径由 `task_id` 确定，无需在 manifest 中保存 locator。prompt 仅可作为冗余审计元数据。
11. task lifecycle event 与 transcript message 写入同一个 task shard，不在 `session.json` 中复制完整事件。
12. `session.json` 不保存 transcript message，也不是 task lifecycle 历史的权威来源。

不提供以下兼容行为：

- 不读取或合并旧 `{agent-id}.jsonl` / `main.jsonl` transcript shard。
- 不读取旧 `session.jsonl` 或独立 `tasks.json`。
- 不进行 dual-write。
- 不根据缺失的 `task_id` 推断 main 或 child identity。
- 不在启动时自动迁移旧 session。

如果现有 session 数据仍需保留，应由独立的一次性离线迁移工具处理；runtime repository 本身只实现新 schema。

### 10.4 Session Manifest

`session.json` 合并原本分散的 session metadata 和 task index：

```json
{
  "schemaVersion": 2,
  "sessionId": "sess_1",
  "cwd": "/project/piko",
  "name": "Agent persistence",
  "createdAt": 1720000000,
  "updatedAt": 1720000100,
  "rootTaskId": "task_root_xxx",
  "activeTaskId": "task_coder_1",
  "defaults": {
    "provider": "openai",
    "modelId": "gpt-5",
    "thinkingLevel": "medium",
    "activeToolNames": []
  },
  "tasks": {
    "task_root_xxx": {
      "agentId": "main",
      "parentTaskId": null,
      "status": "idle",
      "createdAt": 1720000000,
      "updatedAt": 1720000090
    },
    "task_coder_1": {
      "agentId": "coder",
      "parentTaskId": "task_root_xxx",
      "status": "running",
      "createdAt": 1720000050,
      "updatedAt": 1720000100
    }
  }
}
```

权威关系：

```text
session metadata
  → session.json authoritative

task messages and lifecycle history
  → tasks/{task_id}.jsonl authoritative

session.json.tasks
  → rebuildable projection of task shards
```

`session.json` 使用 snapshot 原子替换，而不是 append：

```text
serialize new manifest
  → write session.json.tmp in the same directory
  → flush/fsync
  → atomic rename to session.json
```

### 10.5 Shard Creation and Atomicity

task shard 在 `TaskCreated` durable commit 时创建，初始 user message 随后的 durable commit 写入同一文件：

```text
commit TaskCreated
  → atomically create tasks/{task_id}.jsonl with task header
  → append TaskCreated durable fact
  → update rebuildable session.json task projection
commit initial Message::User
  → append to tasks/{task_id}.jsonl
  → update in-memory projection
```

task header 至少记录：

```rust
pub struct TaskShardHeader {
    pub schema_version: u32,
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub parent_task_id: Option<TaskId>,
    pub created_at: i64,
}
```

repository 打开 shard 时必须校验 header 中的 `session_id/task_id` 与调用参数一致。

shard 创建使用同目录临时文件、flush/fsync 和 atomic rename，防止暴露半个 header。task shard 中的 committed facts 是 task 数据的权威来源；`session.json.tasks` 只是索引，不参与 transcript correctness。

提交顺序和失败语义：

1. 先按 `task_id + task_seq` 或 `message_id` 检查幂等。
2. 将 durable fact append 到 task shard，并完成要求的 flush/fsync。
3. 更新内存投影与 `session.json` manifest。
4. 只有 durable fact 和本次所需投影均可观察后才返回 `PersistAck`。
5. 如果 durable append 成功而投影更新失败，调用不能把该操作当作未发生；重试时 repository 必须识别已经存在的 fact，跳过重复 append，重做投影后返回原 ack。
6. hostd 启动时扫描 `tasks/*.jsonl`，校验 header 和 sequence，并用 shard facts 重建或校正 `session.json.tasks`。孤立但有效的 task shard 不能被静默丢弃。

因此跨文件操作不声称具有文件系统级原子性；一致性来自 authoritative task log、幂等提交和可重建 projection。

### 10.6 Recovery Model

repository 应返回 task-oriented recovery model：

```rust
pub struct RecoveredTask {
    pub metadata: TaskMetadata,
    pub transcript: Vec<CommittedMessage>,
    pub head_message_id: Option<MessageId>,
    pub last_task_seq: u64,
    pub status: TaskStatus,
}
```

```rust
pub trait TaskRepository {
    fn load_task(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> Result<RecoveredTask, StorageError>;

    fn list_tasks(
        &self,
        session_id: &str,
    ) -> Result<Vec<TaskMetadata>, StorageError>;
}
```

恢复流程：

```text
load session.json
  → enumerate task_id
  → open exactly tasks/{task_id}.jsonl
  → validate task shard header
  → load committed message entries
  → validate and order by task_seq
  → reconstruct Message transcript
  → rebuild task head
  → rebuild supervisor handles where required
  → independently project agent view/display history
```

重要原则：

- transcript recovery 消费 `MessageEntry`。
- UI replay 是 `MessageEntry` 的投影。
- transcript recovery 不消费 `DisplayEvent`。
- manifest task metadata 中的 `prompt` 是冗余字段，不是 transcript 唯一来源。

现有 `replay_messages_from_entry` 应在设计上拆成：

```text
recover_transcript_from_entry
project_agent_view_from_entry
```

---

