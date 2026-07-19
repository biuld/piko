# GUI Overlay Stack

> Status: implemented feature contract
> Related: [GUI Workbench](gui-workbench-feature.md),
> [GUI Command Palette](gui-command-palette-feature.md),
> [GUI Chrome Presentation](gui-chrome-presentation-feature.md)
> Design: [GUI Overlay Stack Design](gui-overlay-stack-design.md)

## Overview

The Overlay Stack is the Workbench's chrome-owned layer for focused workflows
that appear above the islands. It dims the Workbench with a backdrop, traps
keyboard focus inside the active overlay, and restores focus when the overlay
closes. Host prompts, local confirms, and transient tools share one stack and
one Escape policy.

## Layout

```text
┌────────────────────────────────────────────────────────────┐
│ TitleBar                                                   │
│ ┌────────────────────────────────────────────────────────┐ │
│ │ Workbench islands (visible, not interactive)           │ │
│ │                                                        │ │
│ │              ┌──────────────────────┐                  │ │
│ │              │ Overlay surface      │                  │ │
│ │              │ title · body · acts  │                  │ │
│ │              └──────────────────────┘                  │ │
│ └────────────────────────────────────────────────────────┘ │
│ StatusBar                                                  │
└────────────────────────────────────────────────────────────┘
```

- The overlay covers the full window content above TitleBar and StatusBar
  chrome as needed; the Workbench remains visible under a dimmed backdrop.
- Overlay panels use elevated surface language consistent with chrome tokens.
- Side Sheets (narrow-window dock fallback) stay separate; they are not center
  overlays. Toast notifications stay separate.

## Behavior and interactions

### Overlay kinds

| Kind | Examples | Dismiss |
|---|---|---|
| HostPrompt | Approval, user interaction | Host resolution only (approval never Escape/backdrop). Interaction Cancel sends an explicit cancel response. |
| LocalConfirm | Busy quit confirm | Cancel / Escape dismisses without quitting; Confirm quits. |
| Transient | Command Palette (later Settings) | Escape or explicit close |

### Priority

HostPrompt outranks LocalConfirm, which outranks Transient. Only one HostPrompt
front is interactive at a time; remaining host prompts show a pending count.
Opening a Transient while a HostPrompt is active is refused. A LocalConfirm
must not replace or cover an active HostPrompt.

### Escape

Escape closes the top dismissable layer in this order:

1. Transient
2. LocalConfirm (as cancel)
3. active side Sheet (restore island focus)

Escape never implicitly accepts or declines an approval.

### Focus

Opening an overlay saves the previous island focus and traps input in the
overlay. Closing restores the saved island, normally the Composer when the user
was writing.

### Existing workflows covered

- Approval and interaction cards from the host prompt queue
- Busy quit confirmation
- Activity Center activation that surfaces the current front prompt
- Command Palette as the first Transient (see Command Palette feature)

## Configuration

No new `[gui]` settings. Physical shortcuts for Transient tools are
frontend-specific (Command Palette uses `Cmd+Shift+P`).

## Non-goals

- Replacing toast notifications or side Sheets with the Overlay Stack
- Sharing TUI `AppMode` / slot overlay semantics
- Client Core ownership of overlay open/close state
- Shipping Settings / Help / Models overlay bodies in this wave

## Acceptance (user-visible)

- Approval and interaction appear as chrome overlays, not library dialogs that
  fight quit confirm for one slot.
- Escape closes Palette or quit confirm without resolving an approval.
- Busy quit confirm does not cover or replace an open approval.
- Closing an overlay returns focus to the previous island.
