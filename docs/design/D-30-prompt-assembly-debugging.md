# D-30: Prompt assembly debugging

> Status: accepted
> Implements: [F-15](../features/F-15-observability.md) (prompt-debugging slice)
> Decisions: [ADR-002](../decisions/ADR-002-codex-modeling-reference.md)

## Goal

Expose a read-only, hostd-authoritative snapshot of the last successful
production prompt assembly for one AgentInstance, plus every actual llmd
model input derived from that assembly.

## Constraints and non-goals

- The capture point must be the production `PromptAssemblyPort`; no parallel
  debug assembler is introduced.
- Snapshots are process-local and latest-only. They are not session facts and
  are never written to the v3 session store.
- Provider-adapter-private HTTP wire JSON remains out of scope. TUI
  presentation is specified separately in the package UI documentation. The
  llmd capture includes the orchd transcript projection and inter-agent
  context once mapped for each model step.
- Prompt bodies must never be copied into tracing fields or log events.

## Proposed design

### Protocol model

`piko-protocol::prompt` adds `PromptDebugSnapshot` with:

- `session_id`, `agent_instance_id`, and exact `run_id` identity;
- the assembled `SemanticRunPrompt`;
- `resource_messages`, ordered as world-state then mention messages;
- the `ResolvedToolCatalog` used by assembly;
- bounded `model_inputs` containing session/agent/run/step identity,
  provider/model, serialized llmd request, and options.

The host command `PromptDebugGet { session_id, agent_instance_id }` returns
`CommandResult::PromptDebugged { snapshot, timestamp }`. The command is
read-only and follows existing command-response correlation.

### Capture and ownership

`OrchAgentRunRunner` owns an `Arc<Mutex<HashMap<(SessionId,
AgentInstanceId), PromptDebugSnapshot>>>`. The latest snapshot remains keyed
by session/agent for lookup, but carries the run id used to reject model inputs
from a replaced run. `HostPromptAssemblyPort` receives that store when the
session execution ports are attached.

```text
orchd resolves tools
  → HostPromptAssemblyPort::assemble_prompt(request)
  → hostd assembles SemanticRunPrompt
  → build snapshot from request resources + tool catalog + result
  → atomically replace map[(session, agent)]
  → return the same SemanticRunPrompt to orchd
```

The capture occurs only after assembly succeeds. Resource-message order is
`world_state` when present followed by `user_mentions`; the user message and
existing transcript are not part of this assembly-level snapshot. Neither
are inter-agent completions, which orchd injects after assembly.

### Read path

`AgentRunRunner` gains a default `prompt_debug_snapshot(session, agent)`
method returning `None`; the orchd adapter overrides it with a cloned map
value. `HostServer` routes `PromptDebugGet` through this port without taking
or mutating `HostState`. Missing values become an explicit
`InvalidCommand("prompt debug snapshot unavailable ...")` response.

### Model-input capture

`GatewayRequest` carries session and AgentInstance identity into llmd. After
llmd maps the semantic prompt and transcript, converts tools, runs pre-chat
middleware, and resolves thinking/cache options, it records the serialized
request and options immediately before opening the provider stream.

The host-provided gateway sink stores these bodies in a separate local mutex
map; it never exports them as metrics or logs. A new successful assembly starts
a buffer for its run id, and each key retains at most 32 model steps. Recording
checks the input run id against the active buffer, so a late step from the
replaced run is discarded. The read path also joins only when the buffer run id
matches the latest assembly clone.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Snapshot DTO plus debug command/result variants |
| `piko-hostd` | Latest assembly and bounded model-input stores, read-only command routing |
| `piko-orchd` | Propagates execution identity on `GatewayRequest` |
| `piko-llmd` | Captures mapped request/options at its provider boundary |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Assembly failure records nothing and preserves the prior successful
  snapshot.
- A missing snapshot is explicit and does not attach a session or start an
  agent.
- Turn cancellation does not remove a captured snapshot: it describes the
  assembly actually used to start that run.
- Concurrent assemblies replace one map entry atomically; sessions and
  agents use distinct keys.
- A model input finishing after its run was replaced cannot contaminate the
  replacement snapshot.
- Restart clears all snapshots by construction.

## Verification

- Protocol serde tests for the new command and result shape.
- Hostd adapter test: production assembly captures prompt, resource ordering,
  and tool catalog exactly; a second assembly replaces only its key.
- Host command test: missing snapshot returns an error; a populated snapshot
  is returned without changing session state or invoking the model gateway.
- Run `cargo fmt --all`, clippy with warnings denied, and workspace tests.

## Alternatives considered

- **Build a standalone ephemeral session like codex-rs.** Rejected for the
  first slice: it duplicates hostd turn preparation and can drift from the
  real tool catalog, world-state baseline, mention resolution, or settings.
- **Persist snapshots in session storage.** Rejected: the transcript already
  owns durable conversation facts, while prompt bodies can contain sensitive
  workspace content and have no restoration value.
- **Expose typed `genai` objects in protocol.** Rejected: JSON values keep the
  wire contract library-neutral while preserving the exact mapped request.
- **Claim provider HTTP-wire equivalence.** Rejected: adapters may apply
  provider-private rendering after this boundary.

## Rollout

1. Protocol DTO and command/result variants.
2. Hostd capture store and prompt-port wiring.
3. Read-only command routing and tests.
4. V-30 evidence and roadmap/status updates.
