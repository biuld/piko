# orchd-api

Public contract for the piko agent runtime. Integrators (such as hostd) depend
on this crate for traits, errors, and port types; the implementation lives in
the [`orchd`](../orchd/) crate.

## Modules

| Module | Content |
|---|---|
| `agent` | `AgentRuntimeApi`, `AgentCommitPort`, recovery and Session ports |
| `execution` | Execution Actor contract, `ExecutionCommitPort`, `RealtimeDeltaSink` |
| `stream` | `SessionSubscription`, `SessionOutputStream` |
| `tools` | `ToolProvider`, execution contexts, `ToolExecResult` |
| `approval` | `ApprovalGateway`, approval request/decision types |
| `error` | `AgentApiError`, `SessionStreamError` |
| `request` / `response` | Subscribe + session runtime snapshot types |

Wire DTOs are defined in `piko-protocol`.

## Usage

```rust
use orchd_api::{AgentRuntimeApi, SessionAgentConfig};
use orchd::AgentRuntime;

let runtime = AgentRuntime::bootstrap(model_executor, config).await;
runtime.attach_agent_session(session_config).await?;
runtime.send_agent_input(request).await?;
```

See the [runtime model](../../docs/single-agent-runtime-model.md),
[Actor design](../../docs/single-agent-actor-runtime-design.md), and
[multi-agent model](../../docs/multi-agent-execution-model.md).
