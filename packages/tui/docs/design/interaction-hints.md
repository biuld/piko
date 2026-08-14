# Interaction Hints Design

## Selected Feature

This design implements
[interaction-hints.md](../features/interaction-hints.md).

## Contract

`ui::interaction_hints::InteractionHints` is a small borrowed value containing
the producer's guidance text. It owns the shared normalization queries used by
projections:

- `is_empty` determines whether any non-empty line exists;
- `single_line` returns the first non-empty line for compact renderers.

The type deliberately contains no `Region`, pane intent, priority, or notice
policy. Those belong to the later projection/arbitration design.

```text
feature interaction state
        │
        └── InteractionHints
                 ├── current Pane footer projection
                 └── future guidance-row projection (not selected here)
```

## Migration

- `PaneFooter::Hints` stores `InteractionHints` rather than an untyped string.
- `PaneSpec::hints` accepts values convertible to the shared contract so
  existing static call sites stay concise.
- Auto-completion providers return `InteractionHints` through their provider
  interface.
- Existing rendering and geometry remain unchanged.

## Verification

- Empty and whitespace-only multi-line declarations reserve no footer row.
- A compact projection selects the first non-empty line.
- Existing pane and Suggest hint rendering remains one row.

