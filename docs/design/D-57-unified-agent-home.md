# D-57: Unified agent home layout

> Status: accepted
> Implements: [F-41](../features/F-41-unified-agent-home.md)

## Goal

Place installed agent specs and host-owned session state below `$PIKO_HOME/agents`
without introducing a second runtime authority or losing existing user data.

## Constraints and non-goals

- hostd remains authoritative for durable session state.
- The append-only session journal and schema-v4 directory contents do not
  change.
- Existing configuration is never overwritten during installation.
- Project-local `.piko/agents` discovery is unchanged.

## Proposed design

The installed catalog loader reads `$PIKO_HOME/agents/spec/*.toml`; development
launchers continue to select checkout resources explicitly. The default JSONL
session repository root becomes `$PIKO_HOME/agents/sessions`.

Before installing catalogs, `scripts/install.sh` migrates the legacy layout.
It first creates `agents/spec`, then considers every legacy top-level TOML in
`agents`. A missing destination is moved, an identical destination removes the
duplicate source, and a differing destination aborts. It next moves each child
of the legacy singular `agent` directory into `agents`; an existing destination
aborts. Finally it removes the singular directory if empty. This ordering makes
the operation repeatable after interruption and keeps collisions recoverable.

The runtime does not perform migration or fallback. Installed binary upgrades
already pass through the installer, keeping filesystem mutation out of hostd
startup and retaining one path authority.

## Package impact

| Package | Change |
|---|---|
| `piko-hostd` | Load installed specs and default sessions from the unified agent home. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- A migration collision exits before either conflicting item is overwritten.
- An interrupted sequence can be rerun: completed moves are absent from the
  legacy source and identical spec duplicates are safely recognized.
- Runtime startup does not silently consult legacy directories.

## Verification

- Unit tests verify installed agent catalog path selection and default session
  root construction.
- The installer integration test verifies fresh layout, legacy migration,
  content preservation, singular-directory cleanup, and collision failure.
- Workspace tests verify project-local overrides remain unchanged.

## Alternatives considered

- Keeping `agent` for sessions was rejected because it preserves the ambiguous
  split the feature removes.
- Runtime fallback was rejected because it creates two authorities indefinitely.
- Silently preferring one side of a collision was rejected because it can lose
  user-authored configuration or history.

## Rollout

1. Document the unified layout and collision policy.
2. Update hostd path resolution and diagnostics.
3. Add installer migration and integration coverage.
4. Migrate the active development installation and run workspace verification.

