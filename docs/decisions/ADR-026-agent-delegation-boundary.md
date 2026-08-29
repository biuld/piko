# ADR-026: Separate agent delegation capability from agent role

> Status: accepted
> Date: 2026-08-29

## Context

piko's named agents have both a permission-oriented `role` and a model-facing
tool allow-list. Neither expresses whether an AgentInstance may create more
agents. Treating the presence of `multi_agent` as that authority lets a
focused worker such as `scout` grow the tree, and hiding the tools alone does
not protect direct runtime callers or stale routes.

## Decision

Introduce the closed `AgentKind` enum on the shared `AgentSpec`:

- `supervisor` may create child AgentInstances;
- `worker` may run work but may not create children.

The kind is independent of `role` and tool-set declarations. Tool discovery
and route execution filter delegation-capable tools for workers, while orchd
authoritatively checks the immutable kind on the parent AgentInstance before
persisting or creating a child. This defense is enforced in orchd because
hostd owns durable state but orchd owns the live agent tree.

New TOML specs that omit `kind` default to `worker`. Pre-existing durable
AgentSpec snapshots that omit the field decode as `supervisor`, preserving
already-created sessions. The complete AgentSpec snapshot remains the source
of truth during recovery; registry changes affect only new instances.
The previous `kind = "leaf"` spelling remains a decode-only alias for `worker`.

## Consequences

- Built-in `main`, `general`, and `coder` remain supervisors; `scout` is a
  worker.
- A supervisor may spawn either kind, subject to the existing session,
  authorization, count, and depth checks.
- A stale or forged delegation route cannot make a worker create a child.
- The protocol gains one durable AgentSpec field but needs no new journal
  event or AgentInstance identity field.
- Future parent-reporting behavior, if needed, can be added as a separate
  capability without re-opening the delegation boundary.

## References

- [F-49: Agent delegation modes](../features/F-49-agent-delegation-modes.md)
- [D-66: Agent delegation modes](../design/D-66-agent-delegation-modes.md)
- [V-63: Agent delegation modes](../verification/V-63-agent-delegation-modes.md)
