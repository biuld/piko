# Design: Composer Queue and Steer

> Status: implemented
> PRD: [../features/message-queue.md](../features/message-queue.md)

## Goal

Give the Composer first-class start / steer / queue / dequeue. Text uses the
scoped command registry; multimodal input uses `ChatSubmitMessage` and
`QueueSteerMessage` with the same host-owned admission semantics.

## Mapping

```text
Enter          idle  → ChatSubmit
Enter          run   → QueueSteer
Alt+Enter      any   → ChatSubmit
Ctrl+Enter     run   → QueueSteer
Ctrl+Enter     idle  → reject (keep draft)
Alt+↑                → pop local follow-up + TurnCancel if turn_id known
```

`AppState::viewed_agent_is_running()` is true when the viewed agent's
`active_turns` status is `Running` (or `Cancelling`).

## Local follow-up stack

`SessionUiState::follow_ups: Vec<FollowUpUi>`

```text
FollowUpUi { agent_instance_id, text, content, turn_id: Option<String>, cancel_when_queued }
```

1. Follow-up `ChatSubmit` pushes `{ text, turn_id: None }`.
2. `TurnEvent::Queued` for that agent fills the oldest unmatched `turn_id`.
   If `cancel_when_queued`, emit `TurnCancel` and drop the row.
3. `Started` / `Cancelled` / `Failed` / `Completed` for that `turn_id` drop
   the row (it left the queue or died).
4. `Queued` must not overwrite a `Running` entry in `active_turns`.

Dequeue pops the last row for the viewed agent. Occupied composer aborts
without mutating the stack.

## Presentation

`AppState::queue_summary()` overlays `follow_ups.len()` onto host
`QueueEvent` counts (hostd currently reports follow-up as 0). Guidance and
the `/agents` detail read that summary. BottomBar stays name-only.

## Files

- `app/turn.rs` — submit / follow-up / steer / dequeue
- `app/event/lifecycle.rs` — bind Queued ids; preserve running turns
- `input/focus/router.rs` + `input/command/` + `input/binding/` — bindings
- `features/guidance_row` — running / dequeue hints
