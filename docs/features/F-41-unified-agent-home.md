# F-41: Unified agent home

> Status: reviewed
> Priority: P1
> Source evidence: piko product direction

## Summary

piko keeps installed agent specifications and durable agent sessions beneath one
agent-owned directory, with separate subdirectories for editable specifications
and runtime state.

## Problem

The user installation currently has sibling `agent` and `agents` directories.
Their names do not communicate why specifications are plural while session
state is singular, and users cannot tell which directory owns agent data.

## User journeys

1. A new user installs piko and sees one agent directory containing clearly
   separated specification and session subdirectories.
2. An existing user reinstalls piko and their custom specifications and durable
   sessions move to the unified layout without being overwritten.
3. An existing user has conflicting old and new specification files. The
   installer stops with a diagnostic and leaves both versions intact.

## In scope

- One user-level directory for installed agent specifications and sessions.
- Automatic, idempotent migration by the installer.
- Preservation of custom specifications and schema-v4 sessions.
- A clear failure for migration collisions.

## Out of scope

- Changing project-local agent override discovery.
- Changing the session journal schema or contents.
- Migrating unsupported pre-v4 session schemas.

## Behavior and states

- A fresh install creates `agents/spec` for editable TOML specifications and
  uses `agents/sessions` for durable session state.
- Reinstall moves legacy top-level agent TOML files into `agents/spec`.
- Reinstall moves the legacy singular agent state directory's children into
  `agents` when their destinations do not exist.
- Byte-identical duplicate specifications are removed from the legacy
  location. Differing files at the same destination fail without overwrite.
- After a successful migration, an empty legacy singular directory is removed.
- Runtime loaders use only the unified paths; there are not two live sources of
  truth.

## Acceptance criteria

- [x] Fresh installation creates agent specifications under `agents/spec`.
- [x] hostd's default session root is `agents/sessions` under `PIKO_HOME`.
- [x] Reinstall migrates legacy specifications and sessions without changing
      their contents.
- [x] A conflicting migration fails and preserves both files.
- [x] Project-local `.piko/agents` overrides continue to load as before.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Unified directory name | `agents` | It already owns the editable catalog and naturally covers multiple agent specifications and instances. |
| Specification location | `agents/spec` | Distinguishes definitions from runtime session state. |
| Collision policy | Fail without overwrite | User-authored files and durable history remain authoritative. |
| Runtime fallback | None | A single live path avoids split authority after migration. |

## Open questions

None for this slice.

## Reference evidence

- F-31 durable session journal
- F-33 local installation

