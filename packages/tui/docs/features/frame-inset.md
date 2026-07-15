# Chat horizontal inset

## Overview

Timeline and AgentPanel keep a one-cell gutter on the left and right so chat
content does not sit flush against the terminal edge. Overlay panels with full
borders, the Editor, BottomBar, and other chrome stay edge-flush.

## Layout

The slot layout still splits the full terminal area. Timeline uses
`[left inset][content][scrollbar]`, treating the scrollbar column as the right
gutter. AgentPanel keeps a full-width top border and insets only the agent rows
below it.

## Behavior and interactions

- Left/right inset applies whenever Timeline or AgentPanel is visible.
- Full overlays, Editor, BottomBar, notifications, and suggestions are not
  inset by this rule.
- On very narrow terminals the horizontal inset shrinks so usable content
  remains.

## Configuration

None in v1. The gutter size is fixed at one cell per side.

## Non-goals

- Global frame inset around every slot
- Top/bottom inset for Timeline or AgentPanel
- Configurable or theme-driven padding
- Changing existing panel-internal padding on bordered overlays
