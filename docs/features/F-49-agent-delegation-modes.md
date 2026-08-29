# F-49: Agent delegation modes

> Status: implemented
> Priority: P1
> Source evidence: piko product direction; F-10 multi-agent; F-19 agent roles; F-21 multi-agent tool surface

## Summary

Every named agent specification declares whether it is a `supervisor` that may
create child agents or a `worker` that may only perform the work assigned to it.
The mode is independent from the permission-oriented `role` field. It controls
both the model-visible delegation surface and the authoritative runtime
permission to create a child AgentInstance.

## Problem

The current agent catalog gives every built-in specification the
`multi_agent` tool set. As a result, a focused researcher such as `scout` can
see and invoke child-agent tools even though its job is to research and report
back. `AgentSpec.role` is already used for permission-profile selection, so it
must not be overloaded with delegation policy. Finally, hiding a tool in the
model catalog alone is insufficient: a direct runtime create request can still
attempt to use any existing parent instance unless the runtime enforces the
same boundary.

## User journeys

1. The root `main` agent spawns `scout` for a research task. Scout receives its
   research and workspace tools, but no delegation tools. It completes and its
   attached or detached report still returns through the existing multi-agent
   result path.
2. A worker agent attempts to create a child through a stale prompt, forged tool
   call, or direct runtime request. No child is created, no durable create
   command is written, and the caller receives a stable
   `agent_cannot_spawn_children` error.
3. A project adds a custom agent without declaring a delegation mode. The
   custom agent is treated as a worker, so adding a new spec cannot accidentally
   widen the agent tree.
4. A project declares a custom supervisor. When its `multi_agent` tool set is
   enabled, it can spawn either worker or supervisor templates. Existing agent
   count and depth limits still bound nested delegation.
5. A user resumes a session after hostd or orchd restarts. The mode captured in
   the AgentSpec snapshot remains the mode of the recovered AgentInstance; a
   changed live catalog does not rewrite the existing instance.
6. A supervisor calls `list_agent_specs`. The catalog lists all templates that
   a supervisor may target, and each entry identifies whether the target is a
   supervisor or worker. A worker target is still spawnable; it simply cannot
   delegate further.

## In scope

- A first-class `AgentKind` with two values: `supervisor` and `worker`.
- An explicit mode in every shipped agent specification:

  | Spec | Role | Mode | Reason |
  |---|---|---|---|
  | `main` | `root` | `supervisor` | Root coordinator and user-facing agent |
  | `general` | `generalist` | `supervisor` | Default general-purpose delegation target |
  | `coder` | `developer` | `supervisor` | Can decompose implementation work |
  | `scout` | `researcher` | `worker` | Focused research worker; reports rather than delegates |

- Configuration and protocol representation of the mode for global, installed,
  and project-local AgentSpecs.
- Capability-aware model tool discovery. Worker agents do not receive the
  delegation tool surface, even if a stale or custom tool-set declaration
  mentions it.
- Runtime enforcement that rejects child creation from a worker AgentInstance.
- Mode visibility in the model-facing AgentSpec catalog.
- Persistence and recovery of the mode through the existing immutable AgentSpec
  snapshot carried by agent creation.
- Compatibility handling for pre-F-49 durable AgentSpec snapshots that do not
  contain a mode.

## Out of scope

- Per-parent allow-lists of child spec IDs.
- New fan-out policy. Count and depth admission semantics are specified by
  [F-50](F-50-configurable-agent-tree-limits.md) and
  [D-67](../design/D-67-configurable-agent-tree-limits.md).
- Permission profiles, sandbox policy, or the meaning of `AgentSpec.role`.
- Dynamic mode changes for an existing AgentInstance.
- A new worker-only reporting or parent-messaging tool. Existing attached,
  detached, and completion-report paths remain unchanged.
- Changes to hostd's durable journal schema or the AgentInstance identity shape.

## Behavior and states

### Mode declaration

`AgentSpec.kind` is the authoritative delegation mode:

- `supervisor`: the instance is eligible to create child agents. It still needs
  the `multi_agent` tool set in its effective model catalog to delegate through
  model tool calls.
- `worker`: the instance may be spawned and run normally, but cannot create child
  agents. The entire current delegation tool surface is absent from its model
  catalog.

The mode is separate from `role`: a role selects permission policy, while kind
selects the agent-tree capability. Two specs may share a role and differ in
delegation mode.

New TOML specs that omit `kind` default to `worker`. Invalid values make that
spec unavailable and produce the loader's normal configuration diagnostic.
Shipped specs declare the value explicitly.

The new serialized spelling is `worker`; the previous `leaf` spelling remains
accepted as a compatibility alias.

For compatibility, a pre-F-49 durable AgentSpec snapshot with no `kind` is
read using the legacy supervisor behavior. Newly loaded TOML specs and newly
created durable instances always carry an explicit mode. This preserves old
session behavior without making omission in a new config a delegation grant.

### Tool surface

The `multi_agent` tool set remains the existing model-facing surface. Its
delegation capability is filtered by the executing AgentSpec kind:

- supervisor + declared `multi_agent` → current multi-agent tools are exposed;
- supervisor without `multi_agent` → no multi-agent tools are exposed;
- worker, regardless of the declared `multi_agent` set → no multi-agent tools are
  exposed.

The catalog remains a catalog of target templates, not a catalog of only
supervisor templates. `list_agent_specs` includes `kind` so a supervising model
can understand the tree shape. The default `general` target remains valid.

### Runtime creation

Child creation requires both:

1. the existing parent/session/tree checks succeed; and
2. the parent AgentInstance's immutable spec mode is `supervisor`.

The child spec may be either kind. A worker can therefore be a normal, valid
child, but it cannot become a parent. A failed worker attempt has no durable
side-effect and returns `agent_cannot_spawn_children`.

### Recovery and lifecycle

Mode is part of the AgentSpec snapshot captured when an instance is created.
Recovery restores that snapshot, so changing a registry TOML file affects only
future instances. Existing attached, detached, queued, completed, closed, and
reopened lifecycle behavior is unchanged apart from the child-creation guard.

## Acceptance criteria

- [x] The four shipped specs expose the mode matrix in this PRD, with `scout`
      as `worker`.
- [x] A fresh custom spec with no `kind` is treated as a worker; an explicit
      `kind = "supervisor"` enables supervisor behavior.
- [x] A supervisor with the effective `multi_agent` set sees the current
      delegation tools, while a worker sees none of them.
- [x] A worker's stale `multi_agent` declaration cannot create a tool route that
      performs delegation.
- [x] A child creation request whose parent is a worker fails with
      `agent_cannot_spawn_children` before any durable create command or child
      AgentInstance exists.
- [x] A supervisor can spawn both a worker target and a supervisor target, subject
      to the existing count/depth/session authorization rules.
- [x] `list_agent_specs` identifies the kind of every target template without
      removing worker templates from the spawn catalog.
- [x] Recovery preserves the stored mode, including a supervisor child nested
      below another supervisor and a worker child below either kind.
- [x] Pre-F-49 durable snapshots without `kind` remain recoverable with their
      legacy supervisor behavior.
- [x] Existing F-10/F-20 attached, detached, report, queue, wait, and lifecycle
      behavior remains green.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What expresses delegation ability? | `AgentSpec.kind` with `supervisor` and `worker` | Makes tree capability explicit and keeps it independent from permission `role` and tool declarations. |
| What is the default for a new config? | `worker` | A new or incomplete spec must not accidentally gain authority to grow the agent tree. |
| What happens to old durable snapshots without the field? | Decode them with legacy supervisor behavior | Preserves established sessions; new TOML loading still defaults safely to worker. |
| Which built-in specs are supervisors? | `main`, `general`, and `coder` | Preserves their current delegation intent while fixing the focused `scout` worker. |
| What does worker hide? | The complete current `multi_agent` tool surface | A worker has no child-management use case; a future parent-reporting need can receive a narrower, separate capability. |
| Is the tool catalog the authority? | No; it is an exposure layer | Runtime child creation must remain safe when tools are stale, forged, or called through another route. |
| Can a worker be spawned? | Yes | Worker describes what the instance may do, not whether a supervisor may assign it work. |
| Can a custom root be a worker? | Yes, if explicitly configured | A root without delegation is a valid constrained product configuration; root tool setup must respect the mode. |

## Open questions

1. Whether a future worker-specific `report_to_parent` tool is needed. It is not
   part of this slice because existing attached/detached completion paths already
   deliver results.

## Reference evidence

- `packages/hostd/resources/agents/main.toml`
- `packages/hostd/resources/agents/general.toml`
- `packages/hostd/resources/agents/coder.toml`
- `packages/hostd/resources/agents/scout.toml`
- F-10 multi-agent and D-10 v2 collaboration tools
- F-19 agent roles and D-22 per-role permission selection
- F-21 model-facing multi-agent tool surface and D-33
- F-41 unified agent home for installed and project-local spec discovery
- [V-63](../verification/V-63-agent-delegation-modes.md) implementation
  verification
