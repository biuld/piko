## 18. Testing Strategy

建议集成测试按行为域组织：

```text
packages/orchd/tests/
├── agent_api/
│   ├── create_task.rs
│   ├── submit_input.rs
│   ├── input_idempotency.rs
│   └── control_task.rs
├── persistence/
│   ├── initial_input.rs
│   ├── steer_input.rs
│   ├── persist_barrier.rs
│   └── local_collector.rs
├── recovery/
│   ├── transcript.rs
│   └── multiple_task_instances.rs
└── multi_agent/
    ├── spawn.rs
    ├── detached.rs
    ├── steer.rs
    └── shared_agent_spec.rs
```

必须覆盖：

1. root initial input 只写一次。
2. child initial prompt 进入 child task transcript 和 JSONL。
3. child steer 进入同一 transcript 和 JSONL。
4. collecting persist/output sinks 能观察 initial input 和 steer，与生产路径事件语义一致。
5. `UserCommitted`/`MessageCommitted` 早于该 work 的 assistant/tool commit。
6. durable barrier 失败时不调用 model gateway。
7. 重复 request/message ID 不重复写盘。
8. 同一 `agent_id` 的两个 task 可分别恢复。
9. 多 task 交错事件不会污染 parent chain。
10. 重开 session 后 per-task transcript 包含 user/assistant/tool 全量消息。
11. recovery 不读取 manifest task metadata 的 `prompt` 补 transcript。
12. hostd 不存在 main-specific append path。

---

