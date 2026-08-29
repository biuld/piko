# Design: Composer Queue and Steer

> Status: implemented baseline; superseded by F-51/D-68 (old commands and
> local follow-up stack are deleted; TUI consumes `AgentWorkSnapshot`)
> PRD: [../features/message-queue.md](../features/message-queue.md)

## Goal

Give the Composer first-class start / steer / queue / dequeue. Under F-51 the
TUI sends one `AgentInputSubmit` with FollowUp or Steer delivery; dequeue
cancels a host AgentInput ID. The `ChatSubmit` / `QueueSteer` / `TurnCancel`
commands and the local follow-up stack are removed.

## Mapping

```text
Enter          idle  → AgentInputSubmit FollowUp
Enter          run   → AgentInputSubmit Steer
Alt+Enter      any   → AgentInputSubmit FollowUp
Ctrl+Enter     run   → AgentInputSubmit Steer
Ctrl+Enter     idle  → reject (keep draft)
Alt+↑                → cancel selected pending AgentInput
```

`viewed_agent_is_busy()` is true when `AgentWorkSnapshot` for the viewed
AgentInstance has active work (including detached work) or a cancelling /
requires-action foreground. It does not read host `active_turns`.

## Queue source

The TUI does not keep `SessionUiState::follow_ups` or overlay local counts onto
host `QueueEvent`. Pending follow-ups, pending steers, and dequeue targets
come from `AgentWorkSnapshot`. Dequeue cancels the selected input ID. Occupied
composer aborts without mutating host state.

## Presentation

`AppState::queue_summary()` reads pending follow-up and steer counts from
`AgentWorkSnapshot`. Guidance and the `/agents` detail read that summary.
BottomBar stays name-only.

## Files

- `app/turn.rs` — submit / follow-up / steer / dequeue
- `app/event/lifecycle.rs` — bind Queued ids; preserve running turns
- `input/focus/router.rs` + `input/command/` + `input/binding/` — bindings
- `features/guidance_row` — running / dequeue hints
