# F-50: Configurable agent-tree limits

> Status: implemented
> Priority: P1
> Source evidence: piko's existing runtime guards and product direction

## Summary

piko exposes the maximum size and nesting depth of each session's AgentInstance
tree through the normal host settings hierarchy. The resolved values are sent
to orchd when the agent runtime is built and govern future child creation,
while existing durable trees remain recoverable.

## Problem

The current agent-count and tree-depth guards are hidden constants in orchd.
Operators cannot tune resource usage per installation or project, and the
configured host state does not describe the limits that the runtime is
enforcing.

## User journeys

1. An operator sets `[agent-runtime] max-agents` and `max-depth` in global or
   project settings.
2. piko starts or rebuilds its orchestrator from the merged settings.
3. A supervisor can create children until either configured limit is reached;
   the next request fails with the existing typed limit error.
4. A user changes the values through the host configuration command; the
   runner is rebuilt and subsequent child creation uses the new values.

## In scope

- A host-owned `[agent-runtime]` settings section with `max-agents` and
  `max-depth` fields.
- Global → project → runtime-override merging, host namespace visibility, and
  persistence through the existing settings manager.
- Defaults that preserve the current behavior: 32 AgentInstances and 8 tree
  levels.
- Protocol runtime fields carrying the resolved limits from hostd to orchd.
- Enforcement on new child creation in the session's AgentInstance tree.
- Explicit count/depth semantics in errors, tests, and documentation.
- Runner rebuild when either setting changes.

## Out of scope

- A separate global process-wide agent quota.
- Per-agent or per-role quotas.
- A change to `max-concurrent-agents`; that existing protocol field remains a
  separate scheduling concern and is not silently reinterpreted here.
- Retrofitting limits onto already durable AgentInstances or deleting an
  existing tree when an operator lowers a setting.
- A new user-facing settings editor; existing ConfigUpdate/ConfigGet surfaces
  are sufficient.

## Behavior and states

### Settings shape and precedence

The canonical TOML shape is:

```toml
[agent-runtime]
max-agents = 32
max-depth = 8
```

The section participates in the existing global settings, project settings,
and in-memory override merge. Each field merges independently, so a project
can change only one limit. `ConfigGet { namespace: "host" }` includes the
resolved section, and normal project persistence retains an update.

### Limit semantics

- `max-agents` is the maximum number of AgentInstances in one session tree,
  including the root. The default is 32.
- `max-depth` is the maximum number of tree levels, including the root. The
  default is 8, allowing the root plus seven descendant levels.
- A value below one is normalized to one at the orchd boundary. This keeps the
  root runnable and makes `1` the explicit root-only setting.
- The limits are checked atomically with child creation under the existing
  session create lock. Failed attempts do not create durable or in-memory
  children.

### Startup, updates, and recovery

At startup, hostd forwards the resolved settings to the runner's
`OrchdConfig.runtime`. A change to either field rebuilds the runner through the
existing runner-frozen-settings observer. The new runtime applies its limits
when sessions are attached and children are created.

Recovery does not reject a durable tree that is already larger or deeper than
the newly resolved limits. The limits are admission controls for new children;
the journal remains authoritative and existing work remains recoverable.

### Failure behavior

When the total count is exhausted, creation returns
`AgentCountLimitExceeded`. When the requested child would exceed the maximum
depth, creation returns `AgentDepthLimitExceeded`. The existing error mapping
and model-facing tool behavior remain unchanged.

## Acceptance criteria

- [x] `[agent-runtime] max-agents` and `max-depth` deserialize using kebab-case
      and appear in the resolved host settings namespace.
- [x] Global, project, and runtime override layers merge the two fields
      independently; defaults are 32 and 8.
- [x] The resolved settings are forwarded to `OrchdConfig.runtime` on startup
      and after a configuration-triggered runner rebuild.
- [x] A session with `max-agents = 2` admits its root and one child, then
      rejects another child without a durable create command.
- [x] A session with `max-depth = 2` admits the root and one child, then
      rejects a grandchild without a durable create command.
- [x] Lowering limits does not prevent recovery of an existing durable tree;
      it only rejects future child creation that would exceed the new limits.
- [x] Values below one are normalized to the root-only effective limit.
- [x] Existing F-10/F-20/F-21 multi-agent behavior remains green with defaults.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What is the settings section? | `[agent-runtime]` | Separates runtime policy from agent spec catalog entries and from scheduling concurrency. |
| Does the count include root? | Yes | It describes the complete session tree and makes a limit of 1 unambiguously root-only. |
| Is depth measured in levels or edges? | Levels including root | It is directly understandable in a tree UI and matches the existing effective default of eight levels. |
| What happens when limits are lowered? | Only future children are blocked | Durable journal recovery must not turn a valid existing session into an unrecoverable one. |
| What is the default? | 32 agents / 8 levels | Preserves current runtime behavior while making it visible and configurable. |
| What does zero mean? | Effective value one | The root must remain runnable; root-only operation is explicitly represented by 1. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Spawn depth/count guards | kept (adapted) | piko keeps fail-closed admission guards but owns their values in host settings and its session-scoped runtime. |
| Runtime-wide scheduling configuration | rejected for this slice | `max-concurrent-agents` is not the same as the per-session tree size/depth policy; piko will model it separately if needed. |

## Open questions

1. Whether a future global process-wide concurrency limit should be wired into
   the same `[agent-runtime]` section or a separate scheduler section.

## Reference evidence

- `packages/orchd/src/runtime/agent/scope.rs`
- `packages/orchd-api/src/error.rs`
- `packages/protocol/src/runtime.rs`
- [F-49 agent delegation modes](F-49-agent-delegation-modes.md)
- [F-10 multi-agent](F-10-multi-agent.md)
- [V-62 configurable agent-tree limits](../verification/V-62-configurable-agent-tree-limits.md)
