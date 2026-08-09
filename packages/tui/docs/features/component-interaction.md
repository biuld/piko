# Component Pointer Interaction

> Status: reviewed
>
> Parent: [component-feedback.md](component-feedback.md)
>
> Related: [pointer-input.md](pointer-input.md)

## Overview

Pointer behavior belongs to the component whose business state gives a hit
meaning. The shared hit map resolves geometry, identity, and paint order; the
pointer router enforces modal authority and delegates; each component decides
what click, wheel, and hover mean for its elements.

## Ownership

| Concern | Owner |
|---|---|
| Rects, element identity, component hit context, pointer gesture, interaction state, z/layer resolution | `piko-tui-layout` |
| Pointer normalization, top-modal barrier, Region dispatch | `piko-tui` pointer router |
| Click/wheel result and hover presentation/style composition | Feature component |
| Host-bound effects and authoritative state | `AppState` action dispatch |

The router may match a `Region` to reach its owner, but must not interpret
feature elements such as workflow choices, tabs, or submit controls.

## Behavior / interactions

- Components receive a resolved component hit with element identity, hit rect,
  absolute coordinates, and component-local coordinates.
- Components may update state they own and return existing keyboard-equivalent
  `Action`s.
- Render receives a generic `InteractionState<E>` from the layout component
  contract. Components paint hover together with selected/active state and own
  their precedence; there is no top-level hover overlay.
- A top modal intercepts pointer events before lower-layer components.
- Clicking outside the top modal follows its surface policy:
  - `Dismiss`: return the same close action as keyboard Esc.
  - `Block`: consume without closing.
- Clicks on `element: None` inside a modal remain component background events;
  they do not implicitly dismiss the surface.

## Surface policy

- Browse, Select, and ordinary Modal surfaces default to `Dismiss`.
- Dock/Decide surfaces (Approval and Tool Interaction) use `Block`.
- No surface currently uses pointer pass-through while it owns modal focus.

## Configuration

No user-facing configuration.

## Non-goals

- Moving business actions into `piko-tui-layout`.
- Making hover mutate keyboard selection.
- Drag, right-click, middle-click, and touch behavior.

## Acceptance criteria

- [x] Pointer routing contains no workflow-choice, approval-decision,
      suggestion-selection, editor-coordinate, notice, or timeline-scroll
      business mapping.
- [x] Current actionable components own their pointer behavior and hover
      feedback.
- [x] Generic component-hit, pointer-gesture, interaction-state, and top-layer
      queries live in `piko-tui-layout`; product Actions remain in `piko-tui`.
- [x] Top-modal outside clicks cannot fall through to plane controls.
- [x] Dismissible surfaces close on outside click; blocking docks do not.
- [x] Keyboard and pointer paths continue to emit the same product actions.
- [x] Shared selectable components expose paint-aligned row geometry while
      feature owners retain activation and safety semantics.
