# Guidance Row

> Status: reviewed
> Design: [guidance-row.md](../design/guidance-row.md)

## Overview

The Guidance Row is the resident, single-line interaction chrome immediately
above the Composer or its ComposerBand replacement. It gives the lower
workspace a stable height and projects either the currently visible notice or
the controls for the active Chat, Select, or Dock interaction.

The row is a Dock Stack anchor. It is not BottomBar shell chrome and it is not
a notification queue. `NotificationCenter` and each interaction surface keep
owning their state; Guidance Row only resolves and paints one projection.

## Layout

```text
Stream
blank boundary row
Suggest?
Guidance Row    resident · exactly one row
Composer        resident, or Select/Dock ComposerBand replacement
BottomBar       shell chrome
```

The order `Suggest → Guidance → Composer` is fixed. Guidance therefore remains
adjacent to the interaction target while Suggest can use it as its footer.

## Content resolution

For the base plane and ComposerBand surfaces, resolve one value per frame:

1. The visible applicable notice, if any.
2. The active Suggest hint while completion is open.
3. The active Select or Dock surface hint.
4. The default Composer hint in Chat. A running viewed agent projects
   `Enter steer · Alt+Enter queue` and live `N steer` / `N queued` counts.
   A waiting follow-up mentions dequeue.

Notice content retains severity styling and its `F8 dismiss` / pointer-dismiss
behavior. Hint content follows the
[Interaction Hints](./interaction-hints.md) contract and remains passive.

CoverBody and Centered surfaces keep pane-local footer hints. CoverBody hides
the underlying plane; Centered guidance must not duplicate or contradict its
local footer.

## Behavior / interactions

- The row participates in every normal Dock Stack solve at height one.
- It never wraps; narrow terminals clip trailing content.
- Notice → hint transitions do not change geometry.
- Clicking the row dismisses only when it currently projects a dismissible
  notice. Hint projections have no hit target.
- `F8` continues to target the currently visible notice regardless of pointer
  availability.
- Opening Suggest, Select, or Dock changes the projected hint immediately.
- Closing the interaction restores the Composer hint.

## Configuration

No setting or binding is added in v1.

## Non-goals

- Moving Guidance into BottomBar or shell chrome.
- Projecting CoverBody or Centered footer hints into the row.
- Combining hint state with `NotificationCenter`.
- Showing more than one notice or hint at once.
