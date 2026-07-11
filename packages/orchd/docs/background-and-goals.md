# Background And Goals


piko 使用 hostd + orchd 架构：

- hostd 是 session storage、用户可见状态和恢复投影的权威层。
- orchd 是 task runtime、transcript mutation、LLM step、工具执行和多 agent supervision 的权威层。
- `agent_id` 标识静态 AgentSpec；`task_id` 标识长生存的 runtime instance。
- main 和所有 child task 共用同一套 orchd 执行链。

当前 user message 路径不对称：

| | main | child task |
|---|---|---|
| 输入来源 | TUI `TurnSubmit` | spawn prompt / `steer_task` |
| 内存 transcript | orchd 写入 | orchd 写入 |
| JSONL | hostd 在 `TurnSubmit` 中特权写入 | 没有统一写入 |
| 恢复 | main 可见 user entry | child 缺少 prompt 和 steer |

根因不是两套 runtime，而是 user transcript mutation 没有统一成为 durable committed fact。

现有 orchd 目录也放大了这一问题：task 创建、输入、transcript、persist 和 lifecycle 横跨 `application`、`runtime/orchestrator`、`runtime/dispatch` 与多个 consumer，缺少一条可追踪的 Agent API 路径。

---

## 2. Goals

本设计目标：

1. main 和所有 child task 使用同一套 Agent API。
2. initial prompt 和后续 steer 使用同一个 task input API。
3. 每次 user transcript mutation 都产生且只产生一个 durable message fact。
4. hostd 不再为 main 维护特殊写盘路径。
5. JSONL message entry 足以按 task 恢复完整 transcript。
6. lifecycle、display、persist 和 command acknowledgement 语义严格分离。
7. task runtime 的输入、状态机、step、tool 和事件输出具有清晰目录边界。
8. 本地测试 sink 与生产 session output hub 共享同一套业务逻辑，不存在 `senders=None` 分支。
9. API 支持幂等、顺序校验、持久化失败和 runtime 恢复。
10. 允许渐进迁移，不要求一次完成存储 schema 大迁移。

非目标：

- 不改变 TUI 的基本交互模型。
- 不改变 spawn、spawn detached、poll 和 steer 的用户语义。
- 不要求立即移除 task metadata 中冗余的 `prompt` 字段。
- 不保留旧 `{agent-id}.jsonl` 存储布局；实现时直接切换到 per-task shard。
- 不把 orchd 变成 durable state authority。

---

