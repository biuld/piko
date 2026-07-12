# orchd-api

Public contract for the piko agent runtime. Integrators (such as hostd) depend
on this crate for traits, errors, and port types; the implementation lives in
the [`orchd`](../orchd/) crate.

## Modules

| Module | Content |
|---|---|
| `execution` | `AgentExecutor`, `ExecutionCommitPort`, session ports |
| `stream` | `SessionSubscription`, `SessionOutputStream` |
| `persist` | `PersistSink` (messages + shard ensure); legacy lifecycle commit types for storage |
| `tools` | `ToolProvider`, execution contexts, `ToolExecResult` |
| `approval` | `ApprovalGateway`, approval request/decision types |
| `error` | `AgentApiError`, `SessionStreamError` |
| `request` / `response` | Subscribe + session runtime snapshot types |

Wire DTOs are defined in `piko-protocol`.

## Usage

```rust
use orchd_api::{AgentExecutor, ExecutionCommitPort, StartExecutionRequest};
use orchd::AgentExecutionRuntime;

let runtime = AgentExecutionRuntime::bootstrap(model_executor, config).await;
runtime.attach_session(session_config).await?;
runtime.start_execution(request).await?;
```

See the [runtime model](../../docs/single-agent-runtime-model.md) and
[landing checklist](../../docs/single-agent-runtime-landing.md).
