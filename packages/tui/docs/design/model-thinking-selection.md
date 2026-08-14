# Design: capability-scoped model configuration

> Status: accepted (implements
> [thinking.md](../features/thinking.md))

## Data flow

```text
ModelList
  → ProviderInfo.models[].reasoningEfforts
  → ModelOption.reasoning_efforts
  → model confirmation (pending only)
  → ThinkingSelector::prepare(selected capabilities)
  → thinking confirmation
  → one ConfigUpdate(provider + model + thinking)
```

`hostd` remains authoritative for the model catalog and durable settings. The
TUI does not infer effort support; it projects the closed capability list
already carried by `ModelSummary`.

## State transitions

The existing `Models` and `Thinking` ComposerBand surfaces form a focus stack.
`AppState.pending_model` bridges the two stages without mutating the active
model. Pushing `Thinking` preserves `Models` underneath, so the normal focus
pop implements Back/Esc. A new model request clears stale pending state.

The final confirmation consumes `pending_model` and emits one JSON Merge Patch
for all three default keys. The active Bottom Bar projection continues to
update from host `ModelEvent::ConfigChanged`, not from speculative local state.

## Capability rules

- Preserve the catalog order supplied by hostd.
- Select the active effort when it occurs in the supported subset.
- Otherwise select the first supported effort and require confirmation.
- Interpret an empty effort list as a non-reasoning target and expose `off`
  only. `off` is accepted by target validation even when reasoning is absent.

## Verification

- The local slash catalog exposes `/model` and omits `/models` and `/thinking`.
- Model confirmation emits no effect and opens the thinking stage.
- The stage contains only the selected target's advertised efforts.
- Final confirmation emits one `ConfigUpdate` with all three keys.
- Empty capabilities produce only `off`.
