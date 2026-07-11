## 20. Required Invariants

实现完成后必须满足：

1. 每个 transcript mutation 对应一个 durable committed message。
2. 每个 committed message 包含 `session_id/task_id/agent_id/message_id/work_id`。
3. 同一 message ID 最多 durable 一次。
4. task 的 LLM step 只读取已经 committed 的 user transcript。
5. initial prompt 和后续 steer 使用同一 input API。
6. main 和 child 使用同一 persistence path。
7. task transcript parent 只指向同一 task 的 message。
8. session tree leaf 不等于 task transcript head。
9. transcript recovery 不依赖 lifecycle prompt 或 display event。
10. supervisor 只管理 runtime handle，不拥有 transcript。
11. hostd 是 durable state authority，orchd 是 transcript mutation authority。
12. `agent_id` 永远不能替代 `task_id` 定位 runtime。
13. production output hub 与 collecting test sinks 不改变业务事件语义。
14. persistence failure 不会产生 LLM side effect。
15. 同一 AgentSpec 的多个 task 可以独立运行、持久化和恢复。
16. 每个 task 恰好对应一个 `tasks/{task-id}.jsonl` transcript shard。
17. `agent_id` 不参与 transcript 文件路由。
18. command 以 `task_id` 为目标，observation 以 `session_id` 为订阅作用域。
19. reliable event 只在对应 durable commit 成功后发布。
20. realtime delta 不参与 persistence 或 recovery，丢失后可由 committed message/snapshot 修正。
21. subscription 的创建、断开或 lag 不得控制或阻塞 task runtime。
22. `turn_id` 与 `work_id` 不互为别名；Turn 属于 session，Work 属于 task。
23. 一个 Turn 可以关联多个 Work；一个 Work 最多引用一个 `source_turn_id`，且允许为空。
24. Turn completion、Work completion 与 Task termination 是三个独立状态转换。

---

## 21. Design Decision Summary

本设计的核心决定是：

```text
“steer”不是一种特殊 transcript API；
它只是向既有 task 调用 submit_input 的场景。
```

```text
“initial prompt”不是 TaskCreated 的隐式字段；
它是 task 的第一条 committed user message。
```

```text
“persist event sent”不等于“durable”；
严格 write-before-LLM 需要 PersistAck barrier。
```

```text
不存在 agent-level transcript shard；
runtime、恢复、排序和物理文件布局都以 task_id 为主键。
```

```text
orchd 的主要阅读路径必须是：
Agent API → application command → task mailbox → commit_input
→ PersistSink → Transcript → step runtime。
```

```text
Agent API 的输出路径必须是：
TaskRuntime → SessionOutputHub → SessionOutputStream
→ hostd → TUI/client；其中 Event 可靠、Delta 临时。
```

这条路径是后续实现和代码评审的主架构约束。
