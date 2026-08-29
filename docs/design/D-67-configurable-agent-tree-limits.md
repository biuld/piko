# D-67: Configurable agent-tree limits

> Status: accepted
> Implements: [F-50](../features/F-50-configurable-agent-tree-limits.md)

## Goal

Carry the resolved host settings for maximum AgentInstance count and tree
depth into orchd, replace the hidden constants in session admission with those
values, and preserve safe recovery when settings change.

## Constraints and non-goals

- hostd owns settings precedence, persistence, and configuration updates.
- orchd owns the live AgentInstance tree and is the final enforcement point.
- The limits apply per attached session tree, not across the whole process.
- Existing defaults remain 32 AgentInstances and 8 levels, including root.
- `max-concurrent-agents` is a separate, currently unconsumed scheduling field;
  this design does not give it new semantics.
- Existing durable sessions are not rewritten or rejected when a limit is
  lowered.

## Proposed design

### 1. Host settings

Add `HostSettings.agent_runtime` with the canonical TOML shape:

```toml
[agent-runtime]
max-agents = 32
max-depth = 8
```

The section uses the existing optional-field settings pattern. The default
settings layer supplies the two current values, and the normal merge function
combines global, project, and in-memory override sections field by field.
`HostSettings::host_namespace_value` exposes the resolved section for
`ConfigGet`; project persistence and JSON Merge Patch already operate on the
same `HostSettings` value.

`AgentRuntimeSettings::to_runtime_config` maps this domain section to the
protocol runtime DTO. No settings state is added to the journal or to the
client protocol.

### 2. Host-to-orchd bootstrap

The production composition path uses a settings-aware runner constructor. It
builds `OrchdConfig.runtime` from `[agent-runtime]` before calling
`AgentRuntime::bootstrap_with_telemetry`. The existing convenience
`new_with_mcp` constructor remains available for tests and embedders and uses
the protocol/runtime defaults.

The config observer treats `agent_runtime` as runner-frozen state. Changing
either limit rebuilds the runner through the existing hostd rebuild path, so
newly attached sessions and future child admission use the new values. The
same path is used for startup and auth/model-driven runner replacement.

### 3. Protocol DTO

Extend `OrchestratorRuntimeConfig` with optional `maxAgents` and `maxDepth`
wire fields:

```rust
pub max_agents: Option<u32>,
pub max_depth: Option<u32>,
```

Optional fields keep older serialized `OrchdConfig` payloads valid. Missing
values resolve to the protocol defaults. The DTO remains serializable data;
it owns no enforcement logic.

### 4. Orchd runtime limits

At bootstrap, `AgentRuntime` resolves `OrchestratorRuntimeConfig` into a
copyable `AgentTreeLimits` value. Absent values use the shared protocol
defaults. Values below one are normalized to one at this boundary, so the
root always remains runnable and `max-agents = 1` / `max-depth = 1` means
root-only operation.

Each `SessionAgentScope` captures the resolved limits at attachment. This
keeps an attached session internally consistent even if a later host config
update builds a new runtime. The scope's existing create lock protects the
count/depth check together with request idempotency and child creation.

The admission algorithm is unchanged apart from reading captured limits:

```text
lock session create state
replay an idempotent request if present
require the parent AgentInstance
reject when current tree count >= max_agents
walk parent ancestry and reject when levels >= max_depth
durably commit Create
insert the live child actor
record the create receipt
```

The check continues to use the current scope map, which includes the root
after attachment and includes recovered AgentInstances. Recovery itself does
not call the admission check; a lower limit therefore cannot strand an
existing session.

### 5. Error and observability contract

The existing `AgentCountLimitExceeded` and `AgentDepthLimitExceeded` errors
remain the stable outcomes. Count exhaustion is checked before depth, matching
the current implementation. A rejected request does not commit a durable
`Create`, insert an actor, or record an idempotency receipt.

No new client command is required. The host's existing error response and
multi-agent tool error mapping surface the typed failure as before.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Add default constants and optional `maxAgents`/`maxDepth` runtime DTO fields. |
| `piko-hostd` | Add `[agent-runtime]` settings, field-wise merge/defaults/namespace exposure, settings-aware runner bootstrap, and runner-rebuild observation. |
| `piko-orchd` | Resolve runtime config into `AgentTreeLimits`, capture it per session scope, and use it for new-child admission. |
| `piko-orchd-api` | No change; existing typed limit errors are sufficient. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Missing runtime fields use the current safe defaults.
- Zero values are normalized to one at the orchd boundary; no session can be
  configured into a non-runnable root state.
- Count/depth rejection is non-retryable for that request and has no durable
  side effect.
- A runner rebuild does not mutate or cancel the old durable tree. Existing
  in-flight work follows the existing runner replacement behavior; a newly
  attached scope captures the new limits.
- Recovery accepts a tree larger/deeper than current settings and applies the
  current limits only to later child creation.

## Verification

- Protocol unit tests verify `maxAgents`/`maxDepth` serialization and omitted
  field compatibility.
- Host settings tests verify default documentation, TOML decoding, field-wise
  merge, namespace exposure, and mapping to the runtime DTO.
- Host config observer tests verify changing `agent_runtime` marks the runner
  as frozen and triggers the existing rebuild path.
- Orchd runtime tests verify a configured count of 2 admits root + one child
  and rejects the next child; a configured depth of 2 admits root + one child
  and rejects a grandchild.
- Existing multi-agent and recovery tests remain green with defaults.

## Alternatives considered

| Alternative | Why it is not selected |
|---|---|
| Keep constants in orchd and document them | Operators still cannot tune resource use and host state remains misleading. |
| Put limits only in the protocol config | There is no user-facing settings source or layer precedence. |
| Put limits in each AgentSpec | Limits protect a session tree and must not vary accidentally by target template. |
| Reject recovery when a new limit is lower | Breaks durable-session recovery and turns a policy change into data loss. |
| Reuse `max-concurrent-agents` | Concurrency and total tree capacity answer different resource questions. |
| Add a new settings command/editor | Existing `ConfigUpdate`/`ConfigGet` already provide the host settings contract. |

## Rollout

1. Add protocol runtime fields/defaults and host `[agent-runtime]` settings.
2. Wire resolved settings through the production runner bootstrap and rebuild
   observer.
3. Replace orchd constants with captured per-session limits.
4. Add settings, protocol, and runtime tests; update the feature index and
   runtime roadmap.
