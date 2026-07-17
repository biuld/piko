# orchd documentation

Package-local Task/Work runtime docs were retired. Normative runtime
documentation lives at the repo root:

| Doc | Role |
|---|---|
| [Single-Agent Runtime Model](../../../docs/single-agent-runtime-model.md) | Concepts and invariants |
| [Single-Agent Actor Runtime Design](../../../docs/single-agent-actor-runtime-design.md) | Tokio Actor realization |
| [Agent Run Atomicity Design](../../../docs/agent-run-atomicity-design.md) | Reliable run startup, completion, follow-up, and detached delivery |
| [Turn–Agent Run Boundary Design](../../../docs/turn-agent-run-boundary-design.md) | hostd Turn completion, Agent report, and observation separation |
| [Multi-Agent Runtime Model](../../../docs/multi-agent-execution-model.md) | AgentInstance Tree, AgentRuntime, AgentActor, tools, and inbox semantics |
| [Tool Sets Design](../../../docs/tool-sets-design.md) | ToolSet grouping, ownership, agent defaults, and LLM catalog rules |

Crate overview: [../README.md](../README.md).
