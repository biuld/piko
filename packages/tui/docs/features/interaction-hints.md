# Interaction Hints

> Status: reviewed
> Design: [interaction-hints.md](../design/interaction-hints.md)

## Overview

Interaction Hints are the shared, placement-independent contract by which an
active interaction surface describes the concise controls that are valid in
its current state. Composer, Suggest, Select, Modal, Approval, and Tool
Interaction surfaces use the same contract even when their hints are painted
in different locations.

The contract separates **what guidance is available** from **where it is
rendered**. The resident [Guidance Row](./guidance-row.md) consumes Chat,
Select, and Dock hints; CoverBody and Centered surfaces consume the same
contract in pane-local footers.

## Content

- A hint is concise, passive guidance for controls that are valid now.
- Actions are ordered from most useful to least useful.
- Empty hints mean that the surface offers no guidance.
- A producer may supply multiple text lines for compatibility, but a
  single-line projection uses the first non-empty line and clips overflow.
- Hint state is derived from the active interaction state and is not persisted.

Examples:

```text
↑/↓ navigate · Enter confirm · Esc cancel
Tab cycle · Enter accept
```

## Behavior / interactions

- Hints have no focus, hit target, or key handling of their own.
- The feature that owns the interaction also owns the hint content.
- Changing interaction steps may change the hint value immediately.
- Rendering a hint must not create a second source of truth for whether an
  action is available.

## Configuration

No settings or bindings are added. Hints describe configured actions; they do
not define the bindings.

## Non-goals

- Defining layout geometry for Guidance Row or pane-local projections.
- Defining arbitration between notices and hints.
- Storing hint history or mixing hints into `NotificationCenter`.
