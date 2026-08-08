# Chat horizontal inset

## Overview

Timeline content and BottomBar chrome share a one-cell gutter on the left and
right so text does not sit flush against the terminal edge. Bordered overlay
panels stay edge-flush (their borders provide the frame).

## Layout

The slot layout still splits the full terminal area. Timeline uses
`[left inset][content][scrollbar]`, treating the scrollbar column as the right
gutter. BottomBar insets its status row with the same
`DEFAULT_HORIZONTAL_INSET` so the agent chip lines up with Stream content.

## Behavior and interactions

- Left/right inset applies to Timeline content and BottomBar.
- Full overlays, Editor, notifications, and suggestions are not inset by this
  rule (Editor keeps its own border chrome).
- On very narrow terminals the horizontal inset shrinks so usable content
  remains.

## Configuration

None in v1. The gutter size is fixed at one cell per side.

## Non-goals

- Global frame inset around every slot
- Top/bottom inset for Timeline
- Configurable or theme-driven padding
- Changing existing panel-internal padding on bordered overlays
