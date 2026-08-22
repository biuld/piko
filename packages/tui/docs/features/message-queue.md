# Composer Queue and Steer

> Status: implemented
> Package: `piko-tui`
> Host/runtime: [F-01](../../../../docs/features/F-01-turn-runtime.md) admission,
> `ChatSubmit` / `ChatSubmitMessage` (FollowUp), `QueueSteer` /
> `QueueSteerMessage` (SteerActive)

## Overview

The Composer can start a turn, inject a steer into the viewed agent's running
turn, or durably queue a follow-up for after that turn. The TUI maps those
intents onto existing host commands. It does not invent a second queue.

## Layout

No new surface. Queue and steer reuse Composer, Guidance Row, agent-status
chrome, and notifications.

Guidance while the viewed agent is running:

```text
Enter steer · Alt+Enter queue · Alt+↑ dequeue
```

Idle Composer keeps `/ commands · @ files · Enter send`. When the viewed agent
has a local follow-up waiting, Guidance mentions `Alt+↑ dequeue`.

## Behavior / interactions

### Submit (Enter)

| Viewed agent | Command | Outcome |
|---|---|---|
| Idle | `ChatSubmit[Message]` | Start a turn |
| Running | `QueueSteer[Message]` | Inject into the active turn at the next model-step boundary |

Slash commands are still intercepted before either path.

### Follow-up (Alt+Enter)

Always `ChatSubmit` (host FollowUp): idle starts a turn; running enqueues a
durable follow-up for the viewed agent.

The TUI records each follow-up locally (structured content and display text, then `turn_id` from
`TurnLifecycle::Queued`) so dequeue can restore it. A `Queued` event must not
replace that agent's running turn in `active_turns`.

### Steer (Ctrl+Enter)

Always `QueueSteer`. If the viewed agent is not running, fail closed: keep the
draft, show an error, send nothing. Hostd also fail-closes when there is no
running turn or orchd rejects the inject; it does not keep a never-drained
steer display queue.

### Dequeue (Alt+↑)

Pops the last follow-up sent from this TUI for the viewed agent.

- Composer empty: restore the text and image references.
- Composer not empty: leave the follow-up queued and report that the composer
  is occupied.
- If the host has assigned a `turn_id`, send `TurnCancel` for that queued
  turn. If `Queued` has not arrived yet, cancel as soon as it does.

Dequeue does not unwind a steer already handed to the running turn.

### Presentation

- Guidance while the viewed agent is running: `Enter steer · Alt+Enter queue`,
  plus `N steer` / `N queued` when those stacks are non-empty.
- Guidance while a local follow-up is waiting: `Alt+↑ dequeue` and the count.
- `/agents` detail repeats the counts. BottomBar does not.
- Status line: `submitted`, `queued`, `steered`, or a fail-closed reason.
  Successful queue/steer do not push a notice (that would hide Guidance).
- Session switch / clear drops the local follow-up stack (host queue remains
  authoritative for later resume).

## Configuration

| Binding ID | Default | Action |
|---|---|---|
| `tui.input.submit` | `enter` | Start or steer |
| `app.message.followUp` | `alt+enter` | Queue follow-up |
| `app.message.steer` | `ctrl+enter` | Steer only (fail if idle) |
| `app.message.dequeue` | `alt+up` | Restore last follow-up |

## Non-goals

- A dedicated queue panel or reordering UI.
- Dequeue of host-only follow-ups this process did not submit.
- Changing orchd admission or adding a new wire command.
- Pre-queueing a follow-up while idle without starting a turn.
