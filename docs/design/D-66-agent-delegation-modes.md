# D-66: Agent delegation modes

> Status: accepted
> Implements: [F-49](../features/F-49-agent-delegation-modes.md)
> Decision: [ADR-026](../decisions/ADR-026-agent-delegation-boundary.md)

## Goal

Introduce a piko-native `AgentKind` boundary between agents that may create
children and agents that are workers. The boundary must be visible during tool
discovery, enforced by the orchd runtime, and retained in the durable AgentSpec
snapshot used for recovery.

## Constraints and non-goals

- `role` remains the F-19 permission-policy identity. It is not consulted for
  delegation authorization.
- `tool_set_ids` remains a model-tool allow-list. It can expose a supervisor's
  delegation tools, but it cannot grant a worker the authority to create a child.
- hostd remains authoritative for loading and distributing AgentSpecs and for
  durable session state. orchd owns the live AgentInstance tree and enforces
  the runtime precondition.
- Existing F-10 tree authorization, queueing, reports, and lifecycle semantics
  remain unchanged. Count/depth admission is governed by
  [F-50/D-67](../features/F-50-configurable-agent-tree-limits.md).
- No new journal event or session schema is required. The existing full
  AgentSpec snapshot in `Create` is extended with the mode.
- The model catalog and the runtime create API are separate defenses. Both are
  required.

## Design

### 1. Shared AgentSpec model (`piko-protocol`)

Add a small closed enum beside `AgentSpec`:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Supervisor,
    #[serde(alias = "leaf")]
    Worker,
}

impl AgentKind {
    pub fn can_spawn_subagents(self) -> bool {
        matches!(self, Self::Supervisor)
    }
}
```

Add `kind: AgentKind` to `AgentSpec`. JSON uses the existing AgentSpec naming
conventions and emits `kind: "supervisor"` or `kind: "worker"`. The decoder
accepts the previous `kind: "leaf"` spelling as a compatibility alias.

The field is part of the spec snapshot, not `AgentInstanceIdentity`: the spec
already travels with `AgentDurableCommand::Create` and
`AgentRecoveryState`. There is no need to duplicate mutable capability data in
the identity.

#### Compatibility decoding

The protocol decoder needs a compatibility default for pre-F-49 durable
snapshots that have no `kind`: decode that legacy representation as
`Supervisor`. This is a compatibility rule for already-created instances, not
the default for new configuration.

The host TOML loader handles new configuration separately: an omitted TOML
`kind` becomes `Worker` before the spec is registered. Thus every new spec sent to
orchd is explicit, while old journal entries remain behaviorally stable.

### 2. Host-owned spec loading (`piko-hostd`)

Extend the TOML agent shape with an optional `kind` field and map it into the
shared enum. The loader continues to accept global installed specs and
project-local `.piko/agents` overrides using the same precedence rules.

The shipped files become:

```toml
# main.toml, general.toml, coder.toml
kind = "supervisor"

# scout.toml
kind = "worker"
```

The loader should warn when a worker explicitly lists `multi_agent` in
`tool_set_ids`, because the declaration is ineffective. It must still retain
the spec rather than silently turn a configuration error into a different
agent. Runtime capability filtering remains the authority for the effective
catalog.

`root_agent_spec` and `ensure_root_tool_sets` must add the mandatory
`multi_agent` set only when the root spec is a supervisor. A custom root that is
explicitly a worker remains runnable but has no delegation surface.

The existing host `AgentSpecList` command automatically carries the field as
part of `AgentSpec`. No separate host-only capability table is introduced.

### 3. Trusted kind in tool discovery and execution (`piko-orchd-api`)

The model catalog is built before a run and tool execution later receives a
trusted runtime context. Carry the resolved kind through both contexts:

```rust
pub struct ToolDiscoveryContext {
    // existing fields …
    pub agent_kind: AgentKind,
}

pub struct ToolExecutionContext {
    // existing fields …
    pub agent_kind: AgentKind,
}
```

The fields are runtime identity, never model-controlled arguments. A missing
field in an old serialized context defaults to `Worker` for fail-closed behavior;
normal orchd construction always supplies the AgentSpec value.

`ExecutionIdentity` carries the same kind so tool-batch context construction
cannot accidentally fall back to a registry lookup. For a recovered Agent,
`StartExecutionRequest.agent_spec` is the durable snapshot and is the source of
truth for kind.

### 4. Capability-aware catalog filtering (`piko-orchd`)

The existing multi-agent tool definitions already carry
`ToolCapability::Delegation`. Extend the registry's catalog filtering so the
same predicate is applied to both returned tool definitions and route entries:

```text
tool_allowed(tool, agent_kind):
  if agent_kind == worker and tool.capabilities contains Delegation:
    false
  else:
    true
```

This is applied after tool-set expansion and alongside the existing feature
and active-name filters. Filtering both lists and routes is required; otherwise
a worker could receive a hidden route through a stale catalog.

For the normal path, `ExecutionServices::prepare_run_context` supplies kind
from the AgentSpec being prepared. A custom tool set that references the
multi-agent provider is still covered because the filter is based on the
tool's capability, not only the literal `multi_agent` set ID.

`MultiAgentToolProvider::execute` also checks the trusted execution context and
returns `agent_cannot_spawn_children` for a worker delegation call. This protects
direct provider tests and future routes that do not pass through the normal
catalog path. It is defense in depth; the runtime create guard below is still
mandatory.

The `list_agent_specs` value includes `kind` in each entry:

```json
{
  "specs": [
    {"id": "scout", "name": "Scout", "role": "researcher", "kind": "worker"},
    {"id": "coder", "name": "Coder", "role": "developer", "kind": "supervisor"}
  ],
  "default_spawn_spec_id": "general"
}
```

The tool descriptions should say that the catalog contains target templates,
that `kind` describes whether a target may delegate, and that worker agents do
not receive the delegation surface themselves.

### 5. Authoritative child-creation guard (`piko-orchd`)

`create_agent` currently checks the parent exists, idempotency, session, and
tree limits, then resolves the target spec. Add the kind check after the
idempotent replay path and parent lookup, before tree validation or durable
creation:

```text
parent = scope.agent(parent_agent_instance_id)
if parent.kind != Supervisor:
    return AgentCannotSpawnChildren
```

`AgentHandle` stores the kind captured when its actor is created. The value is
derived from the actor's resolved AgentSpec, so a recovered actor uses its
durable snapshot rather than a newly changed registry entry. A
`SessionAgentScope::can_spawn_children` helper keeps the API implementation
from reaching into actor state.

Add `AgentApiError::AgentCannotSpawnChildren`, mapped by the multi-agent tool
adapter to the stable model error code `agent_cannot_spawn_children`. The failed
path must not call the commit port, insert a child handle, or consume a count or
depth slot.

The target spec is resolved only after the parent capability check. A target's
own kind is not a reason to reject creation: supervisors may spawn workers and
supervisors alike.

### 6. Durable recovery

No new `AgentDurableCommand` variant is needed. The existing `Create` command
already stores the complete `AgentSpec`, and recovery already restores that
spec into `AgentRecoveryState` before constructing the actor. New creates write
the explicit `kind`; old creates decode through the compatibility rule in §1.

The live registry remains authoritative only for new AgentInstances. Recovery
continues to prefer the stored spec, preserving both the mode and the existing
F-48 immutable recovery identity contract.

### 7. Existing limits and authorization

The kind check does not replace `authorize_input` or the configurable count and
depth limits from [F-50/D-67](../features/F-50-configurable-agent-tree-limits.md).
The order for a new model spawn is:

```text
model catalog exposure
        ↓
trusted caller/parent lookup
        ↓
parent kind == supervisor
        ↓
existing session, idempotency, authorization, count, and depth rules
        ↓
target spec resolve + durable Create + actor start
```

The tool surface normally prevents a worker from reaching the create path, but
the runtime check is the final authority for all callers.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Add `AgentKind`; add `kind` to `AgentSpec`; carry kind in trusted tool contexts if those contexts remain protocol DTOs; preserve legacy decode. |
| `piko-hostd` | Parse TOML `kind`, assign new-config worker default, update built-in specs, respect kind in root tool setup, and warn on ineffective worker delegation declarations. |
| `piko-orchd-api` | Expose trusted `agent_kind` in discovery/execution context as needed and add the child-capability error to `AgentApiError`. |
| `piko-orchd` | Filter delegation tools for workers, guard direct provider calls, retain kind in execution identity/handles, and reject worker child creation. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Invalid TOML kind: the affected spec is rejected with the loader's existing
  diagnostic path; other specs continue to load.
- Worker spawn attempt: returns a non-retryable
  `agent_cannot_spawn_children` error, with no durable or in-memory child
  creation side effect.
- Stale worker tool route: provider and runtime checks fail closed even if a
  caller bypasses normal catalog discovery.
- A supervisor without `multi_agent`: no delegation tools are exposed, but a
  trusted non-model caller may still use the existing create API subject to
  all runtime checks.
- Session detach, cancellation, queueing, report delivery, and actor shutdown:
  unchanged from F-10/F-48. The kind is immutable for the actor lifetime.
- Legacy snapshot with missing kind: compatibility decode yields supervisor;
  the next newly created instance uses the explicit current registry spec.

## Verification

- Protocol unit tests serialize both kinds and decode a pre-F-49 AgentSpec with
  the legacy compatibility default.
- Host loader tests cover explicit values, omitted new-config values, invalid
  values, and the four built-in mode assignments.
- Root tool-set tests prove a supervisor root receives `multi_agent` and an
  explicitly worker root does not receive it.
- Registry tests prove delegation definitions and routes are present for a
  supervisor and absent for a worker, including a custom set that references the
  provider.
- Multi-agent provider tests prove direct worker spawn attempts return the stable
  error.
- Runtime tests prove a worker parent cannot create a child and that no commit
  or handle is produced; supervisors can create both target kinds.
- Recovery tests prove the stored mode wins over a changed live registry.
- F-10, F-20, F-21, and existing tool-set/catalog tests remain green.

## Alternatives considered

| Alternative | Why it is not selected |
|---|---|
| Remove `multi_agent` only from `scout.toml` | Fixes one shipped file but leaves custom specs, stale routes, and direct runtime create requests unrestricted. |
| Infer delegation from `role` | Couples permission policy to tree authority and makes user-defined roles ambiguous. |
| Treat `tool_set_ids` as the security authority | Tool exposure is not sufficient for a caller that can reach the runtime API through another route. |
| Add only a boolean `can_spawn_subagents` | A named mode is clearer in catalogs and leaves room for future agent-kind behavior without making the role field carry it. |
| Default every missing mode to supervisor | Preserves old config but grants new incomplete specs tree authority; only legacy durable decoding needs that compatibility behavior. |
| Give workers a reduced multi-agent tool subset now | No current worker workflow needs it; a future parent-reporting action should have a narrow, explicit capability rather than reopening child management. |

## Rollout

1. Add `AgentKind`/`AgentSpec.kind`, compatibility decoding, loader parsing, and
   explicit built-in TOML values.
2. Thread trusted kind through run preparation and make the registry filter
   delegation capabilities for worker agents.
3. Add provider defense-in-depth errors and the authoritative runtime parent
   kind check.
4. Update catalog descriptions and host/TUI-facing spec assertions.
5. Add protocol, loader, catalog, runtime, and recovery verification, then
   update the F-21/named-agent documentation to reference F-49.
