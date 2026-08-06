# F-18: Managed feature gating

> Status: implemented (F-18/D-21/V-21)
> Priority: P1
> Source evidence: codex-rs `features/src/*` (`Features`, feature catalog,
> stability tiers, legacy aliases), `core/src/config/managed_features.rs`
> (`ManagedFeatures`, pinned-feature constraints),
> `core/src/config/requirements.rs` (`FeatureRequirementsToml`), digest
> Block M (config layers, managed features)

## Summary

Operators can gate tool availability project-wide through a `[features]`
settings section. Named feature keys map to piko tool families; a disabled
feature removes its tools from the model-visible catalog, and a direct call
to a disabled tool fails closed with a deterministic `feature_disabled`
error. A `managed` sub-table pins features to a fixed value across all
settings layers, so an operator-declared policy cannot be weakened by a
global setting or a runtime override. With no `[features]` section, every
tool family stays enabled exactly as today.

## Problem

1. **Tool enablement is per-session and reactive, not configured.** Today a
   tool family (`process`, `bash`, MCP tools, multi-agent tools, …) is
   either fully enabled or restricted through transient
   `active_tool_names` on a per-run basis. There is no settings-native way
   to say "this project never exposes long-lived processes" or "MCP tools
   are off here" so it applies to every session and every user of the
   project.
2. **Nothing pins tool policy against weaker layers.** A project cannot
   declare a tool policy that survives user/runtime overrides. Even if a
   project disabled a feature, a lower-authority layer could silently
   re-enable it, defeating the operator intent.
3. **The model sees tools it can never use.** Disabled-by-policy tools are
   still listed to the model and still route to approval prompts. The
   approval gateway can deny commands (F-17) but has no availability gate
   before the model decides to call a tool family that policy excludes.

## User journeys

1. A project adds `[features] process = false` to `.piko/settings.toml`.
   Every session in the project is booted without the `process` tool: the
   model never sees it in the catalog, and a direct
   `process start` call fails with `feature_disabled` instead of prompting.
2. Global settings declare `[features] mcp = false`; a project tries
   `[features] mcp = true`. The project also pins
   `[features.managed] mcp = false`. hostd logs a warning naming the
   conflicting setting, resolution keeps `mcp` off, MCP servers are not
   connected, and no MCP tool appears in any catalog.
3. An operator pins `[features.managed] process = false`; a runtime
   override sets `process = true`. The pin wins: the warning is logged and
   the feature resolves off, so the override cannot weaken operator policy.
4. A user with no `[features]` section runs piko. Behavior is unchanged:
   all tool families are enabled and the catalog is identical to today.
5. An operator typo's a key (`[features] proces = false`). hostd warns and
   ignores the unknown key; no tool family is affected (unknown keys can
   never disable anything).

## In scope

- `[features]` settings section with two maps:
  - `enabled`: named feature key → bool, merged per key across
    global/project/override (override wins per key);
  - `managed`: named feature key → bool pins, merged per key across layers;
    pins are the final authority over `enabled` in every layer.
- Built-in feature catalog with stable keys mapped to tool families:
  `workspace` (read/edit/write), `bash`, `process`, `environment`,
  `context` (get_context_remaining / new_context_window), `todo`
  (todo_read / todo_write), `multi-agent` (spawn/followup/interrupt/list/
  wait family), `user-interaction` (ask_user / request_user_input), and
  `mcp` (every MCP-server tool, identified by its `mcp` executor kind).
  Default: all features enabled.
- Resolution at session start (hostd owns settings): merged `enabled` map
  plus merged `managed` pins produce a deterministic `ResolvedFeatures`
  DTO. A pinned key's resolved value is the pin; an explicit `enabled`
  value that contradicts the pin logs a warning naming the feature and the
  expected value. Unknown keys in either map log a warning and are
  ignored.
- Tool gating in orchd:
  - catalog filter: a tool whose feature is disabled is excluded from both
    the discovered `ToolDef` list and the execution route map, so the model
    never sees it and it cannot be executed;
  - direct execution of a disabled tool fails closed with a non-retryable
    `feature_disabled` error naming the feature;
  - `active_tool_names` still intersects after the feature filter.
- MCP integration: when `mcp` is disabled, hostd skips connecting MCP
  servers at session bootstrap (no server processes) and the catalog filter
  still denies any MCP tool that somehow registers.
- `resources/settings.default.toml` documents the section.

## Out of scope

- Agent-role layers (per-role permission-profile selection) — next M-config
  slice.
- Dependency normalization between features (families are orthogonal; no
  dependency exists, so nothing cascades).
- Changing tool approval tiers (`never` / `on-request` / `always`),
  sandbox execution whitelists, or permission profiles from features.
- Dynamic feature switching mid-session; features resolve once per session
  start from the merged settings at that time.
- Stability tiers, legacy aliases, and per-feature schemas from the codex-rs
  `features` crate (no piko consumer for any of them).

## Behavior and states

### Feature resolution

```text
[features] sections: global → project → override (per-key merge)
  enabled: per-key override (higher layer wins per key)
  managed: per-key override (higher layer pin wins per key)
  resolved[key] =
    pinned[key]          if key is pinned          (pin wins, warn on conflict)
    enabled[key]         if key is explicitly set
    true                 otherwise                 (catalog default)
  unknown key (either map) → warn + ignore
  no [features] section  → all features enabled
```

- A pin is the final authority: the merged `enabled` value for a pinned key
  is ignored in favor of the pin, and a contradiction is surfaced through a
  startup warning (deterministic, fail-closed: the disabled state is
  enforced regardless of who set what).
- Unknown keys cannot disable anything: they are ignored with a warning, so
  a typo never silently locks down (or unlocks) a tool family.

### Tool gating (orchd)

```text
catalog build
  ├─ feature filter: tool.feature is enabled ──> included (discovery + route)
  └─ tool.feature is disabled ─────────────────> excluded (never visible,
                                                   never executable)

direct call to a disabled tool (no route)
  └─ non-retryable feature_disabled error: "tool 'process' is disabled by
     feature 'process'"
```

- Feature gating runs before approvals: a disabled tool never reaches the
  approval gateway, so it never prompts.
- `active_tool_names` (session/agent restriction) is applied after the
  feature filter; both must allow a tool for it to appear.
- Unmapped tools (a tool name not in the catalog and not an MCP executor)
  are not gated by any feature; they behave as today.

### Races

- Feature resolution vs. session start: resolution is synchronous at
  session bootstrap and completes before any catalog build or tool call.
- Pin vs. override: the pin is applied at resolution time; there is no
  runtime mutation surface, so no mid-session race exists.

## Acceptance criteria

- [x] `[features]` merges per key across global/project/override for both
      `enabled` and `managed`; absent sections resolve to empty maps
      (fixture: settings merge unit tests, defaults template check).
- [x] With no `[features]` section, every feature resolves enabled and the
      catalog is unchanged (fixture: hostd resolver test + orchd catalog
      test with default config).
- [x] `[features] process = false` removes `process` from the discovered
      tools and the route map; a direct `process start` call fails with a
      non-retryable `feature_disabled` error naming the feature (fixture:
      orchd registry test).
- [x] A pinned feature wins over an explicit `enabled` value in the same or
      a higher layer: `enabled.process = true` + `managed.process = false`
      resolves off and logs a warning (fixture: hostd domain resolver
      tests).
- [x] Unknown feature keys warn and are ignored; they disable nothing
      (fixture: hostd domain resolver test).
- [x] MCP tools (executor kind `mcp`) are gated by the `mcp` feature: with
      `mcp` disabled they are absent from the catalog even when an MCP
      server registers (fixture: orchd registry/catalog test).
- [x] `active_tool_names` still intersects with the feature filter: a tool
      enabled by features but not in `active_tool_names` stays hidden
      (fixture: orchd registry test).
- [x] Non-feature behavior is unchanged: approvals, safety, guardian, and
      permission command rules still apply to enabled tools (fixture:
      existing hostd gateway + registry tests green).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does feature resolution run? | hostd, consumed as a DTO by orchd | hostd owns settings (F-17 precedent); orchd only applies the resolved feature set to the catalog |
| Pin conflict: hard error or pin wins? | Pin wins + startup warning | piko merges layers and already surfaces unresolvable config as warnings (unknown profile → default); a hard startup error would let a stale user override brick a pinned project |
| Unknown feature key | Warn + ignore | Mirrors F-17 unknown-profile handling; a typo cannot disable anything (safe direction) and cannot weaken policy |
| What happens to MCP servers when `mcp` is off? | Skip connecting + catalog gate | No server processes when the feature is off (fail-closed, cheaper); the catalog gate is defense in depth for any stray registration |
| Gate before or after approvals? | Before | A disabled tool must never prompt; availability is a precondition of the approval flow |
| Feature dependencies | None in this slice | Tool families are orthogonal; dependency normalization lands only when a real dependency exists |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `Features` named feature flags with defaults | **kept (adapted)** | Stable piko catalog keys mapped to tool families, all enabled by default; no behavior change without a `[features]` section |
| Feature stability tiers, legacy aliases, per-feature schemas | **rejected** | No piko legacy keys or consumers; the catalog is a fixed small set documented in one table |
| Dependency normalization (`normalize_dependencies`) | **rejected (deferred)** | Families are orthogonal; normalization lands with the first real dependency |
| `FeatureRequirementsToml` managed pins | **kept (adapted)** | `[features] managed` pins in the settings layers; pin wins over every layer with a conflict warning |
| `ManagedFeatures` constrained mutation | **kept (adapted)** | No runtime mutation surface exists in piko, so the constraint reduces to a pure resolve function with unit-tested pin semantics |
| Startup rejection of contradictory feature settings | **kept (adapted)** | Contradictions surface as warnings and the pin wins instead of a hard startup error; deterministic and fail-closed |

## Open questions

1. Should a later slice let agent roles (F-19) or permission profiles select
   per-agent feature sets? Deferred until the agent-role slice lands.

## Reference evidence

- codex-rs `features/src/lib.rs` (`Feature` catalog, defaults,
  `FeatureConfig`), `features/src/feature_configs.rs`, `features/src/tests.rs`.
- codex-rs `core/src/config/managed_features.rs` (`ManagedFeatures`,
  pinned-feature normalization and validation).
- codex-rs `core/src/config/requirements.rs`
  (`FeatureRequirementsToml`).
- piko `packages/hostd/src/domain/config/settings.rs` (merge machinery),
  `packages/hostd/src/protocol/orch_factory.rs` / `adapters/turns/orch_runner/mod.rs`
  (session bootstrap), `packages/orchd/src/adapters/tools/registry.rs`
  (catalog filter), `packages/protocol/src/config.rs` (`OrchdConfig`).
