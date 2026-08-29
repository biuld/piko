# Composer Queue and Steer

> Status: superseded by F-51/D-68 (direct TUI cutover; old commands deleted)
> Package: `piko-tui`
> Host/runtime: [F-51](../../../../docs/features/F-51-agent-control-plane.md)
> `AgentInputSubmit` (FollowUp / Steer), `cancel_input`, `AgentInterrupt`

## Overview

The Composer starts work, steers the viewed agent's active root (including
detached work), or queues a follow-up. The TUI maps those intents onto
`AgentInputSubmit` and reads queue/steer state only from `AgentWorkSnapshot`.
It does not keep a local follow-up stack and does not send `ChatSubmit`,
`QueueSteer`, or `TurnCancel`.

## Layout

No new surface. Queue and steer reuse Composer, Guidance Row, agent-status
chrome, and notifications.

Guidance while the viewed agent is running:

```text
Enter steer · Alt+Enter queue · Alt+↑ dequeue
```

Idle Composer keeps `/ commands · @ files · Enter send`. When the viewed agent
has a host pending follow-up, Guidance mentions `Alt+↑ dequeue`.

## Behavior / interactions

### Submit (Enter)

| Viewed agent | Command | Outcome |
|---|---|---|
| Idle | `AgentInputSubmit` FollowUp | Apply as root AgentInput; start work |
| Running | `AgentInputSubmit` Steer | Bind to the active root; apply at the next ModelStep |

Slash commands are still intercepted before either path.

### Follow-up (Alt+Enter)

Always `AgentInputSubmit` FollowUp: idle applies a root AgentInput; running
admits a durable pending follow-up for the viewed agent. The TUI reconciles
the receipt by input ID against `AgentWorkSnapshot`.

### Steer (Ctrl+Enter)

Always `AgentInputSubmit` Steer. If the viewed agent has no active root, fail
closed: keep the draft, show an error, send nothing. hostd also fail-closes
when there is no active root or orchd rejects the bind.

### Dequeue (Alt+↑)

Cancels the selected pending AgentInput for the viewed agent (last admitted
follow-up in the snapshot).

- Composer empty: restore the text and image references, then `cancel_input`.
- Composer not empty: leave the follow-up queued and report that the composer
  is occupied.
- Identity is the host input ID, never a display index or `TurnCancel`.

Dequeue does not unwind a steer already applied to a ModelStep.

### Presentation

- Guidance while the viewed agent is running: `Enter steer · Alt+Enter queue`,
  plus `N steer` / `N queued` when those stacks are non-empty.
- Guidance while a host pending follow-up is waiting: `Alt+↑ dequeue` and the count.
- `/agents` detail repeats the counts. BottomBar does not.
- Status line: `submitted`, `queued`, `steered`, or a fail-closed reason.
  Successful queue/steer do not push a notice (that would hide Guidance).
- Session switch / clear drops only editor optimism; host `AgentWorkSnapshot`
  remains authoritative on resume.

## Configuration

| Binding ID | Default | Action |
|---|---|---|
| `tui.input.submit` | `enter` | Start or steer |
| `app.message.followUp` | `alt+enter` | Queue follow-up |
| `app.message.steer` | `ctrl+enter` | Steer only (fail if idle) |
| `app.message.dequeue` | `alt+up` | Restore last follow-up |

## Non-goals

- A dedicated queue panel or reordering UI.
- Reordering the host follow-up list.
- Pre-queueing a follow-up while idle without starting a Run.
