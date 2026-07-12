# orchd-api

Public contract for the piko agent runtime. Integrators (such as hostd) depend on this crate for traits, errors, and port types; the implementation lives in the [`orchd`](../orchd/) crate.

## Modules

| Module | Content |
|---|---|
| `runtime` | `AgentRuntime` trait |
| `stream` | `SessionSubscription`, `SessionOutputStream` |
| `persist` | `PersistSink`, commit/ack types |
| `tools` | `ToolProvider`, execution contexts, `ToolExecResult` |
| `approval` | `ApprovalGateway`, approval request/decision types |
| `input` | `build_user_input()` helper |
| `error` | `AgentApiError`, `SessionStreamError` |
| `request` / `response` | Re-exports from `piko-protocol` |

Wire DTOs are defined in `piko-protocol`.

## Usage

```rust
use orchd_api::{AgentRuntime, PersistSink, ToolProvider};
```

Construct and wire a runtime via `orchd::Runtime`:

```rust
use orchd::Runtime;

let runtime = Runtime::bootstrap(model_executor, config).await;
runtime.set_persist_sink(persist_sink).await;
let agent_runtime = runtime.agent_runtime(); // AgentRuntimeService
```

The current Task/Work API is scheduled to migrate to the Execution-centered
contract described in the [runtime model](../../docs/single-agent-runtime-model.md)
and [migration plan](../../docs/single-agent-runtime-migration.md).
