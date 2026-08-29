# Agent-Directed Chat Design

## Status

Implemented; wire and lifecycle superseded by F-51/D-68.

## Scope

The Editor submits text to the concrete AgentInstance selected in
`AgentPanelState`. Root and child AgentInstances use the same
`AgentInputSubmit` command and host work projection.

This feature changes command routing and lifecycle projection. It does not add
a Panel, Slot, overlay, focus target, setting, or key binding.

## User-visible contract

- Timeline shows the selected AgentInstance transcript.
- A newly created Session starts with its root AgentInstance selected, so the
  first Editor submission has a concrete target without manual selection.
- Enter captures the selected `agent_instance_id` and sends the text to that
  target.
- A user-origin start or follow-up is an AgentInput row; steer joins the
  current root.
- One AgentInstance has at most one active root. Later FollowUp inputs stay
  pending in admission order.
- Different AgentInstances in the same Session may run concurrently.
- Streaming output and committed messages remain scoped to the target
  AgentInstance.
- Switching Agent selection after submission does not retarget the accepted
  input.
- Esc interrupts the current work for the AgentInstance currently shown.

## Wire contract

The TUI sends `AgentInputSubmit` for every Editor submission (FollowUp or
Steer). `ChatSubmit` is deleted.

```rust
Command::AgentInputSubmit {
    command_id,
    session_id,
    agent_instance_id,
    delivery, // FollowUp | Steer
    content,
}
```

The selected AgentInstance is explicit. hostd does not infer the target from
root identity, AgentSpec ID, display name, or current server-side selection.

User-origin start and follow-up rows are AgentInputs in the host work
projection. The TUI does not keep `active_turns` as control state and
does not consume `TurnEvent` as a second lifecycle.

`AgentChanged` remains authoritative for AgentInstance lifecycle. Foreground
and pending work come from `AgentWorkSnapshot`.

## TUI state

Composer routing and cancellation use `AgentWorkSnapshot` for the viewed
`AgentInstance`. Timeline grouping is presentation of user-origin AgentInputs
and their root.

`SessionReconciled` replaces work state from the host snapshot. Incremental
realtime items may be optimistic; the next snapshot is truth.

## hostd flow

Protocol dispatch calls `AgentWorkControl::submit`. There is no
`apply_chat_submit` / `start_turn` write path.

Every user Composer submit is an `AgentInput` with `FollowUp` or `Steer`
delivery. `AgentActor` admits it through `submit_agent_input` and commits
before mutating actor state.

hostd invokes one runner interface:

```rust
AgentRunRunner::run_agent(AgentRunInput) -> AgentRunHandle
```

`AgentRunHandle` includes the `AgentInputReceipt` that tells hostd whether the
input started or was queued, a `started` receiver that yields its
`SessionSubscription`, and its completion receiver. For a user-origin Run,
optional `user_turn_id` is correlation on the input, not a parent aggregate.

`AgentActor::advance_next_follow_up` starts queued input after the prior root
is terminal. hostd validates the addressed AgentInstance and refreshes
`AgentWorkSnapshot` from the journal facts.

## Transcript and observation

orchd commits each message to the target AgentInstance shard. hostd projects
reliable commits and realtime deltas with their `agent_instance_id`; the TUI
only applies them to the matching Agent view.

The completion channel determines the business result. Observation is lossy;
query paths recover from `AgentWorkSnapshot`.

## Cancellation and compaction

Dequeuing a pending follow-up sends `cancel_input` with the AgentInput ID.
Esc sends `Command::AgentInterrupt` addressed by
`session_id + agent_instance_id`. `TurnCancel` is deleted.

`Command::SessionCompact` includes an explicit `agent_instance_id`. Current
`SessionTreeEntry` compaction is applied only to the root AgentInstance; hostd
does not accidentally compact root state when a child is selected.

## Error handling

- An unknown, closed, or unavailable target rejects the command.
- A busy target admits a FollowUp as pending instead of redirecting or
  returning a root-specific error.
- A mismatched `AgentRunReport.agent_instance_id` fails the operation.
- Session reconciliation replaces stale local work state from the host
  snapshot.

## Non-goals

- Sending one Editor submission to multiple AgentInstances.
- Reopening a closed AgentInstance automatically.
- Changing Agent hierarchy or authorization policy.
- Adding new TUI layout or focus behavior.
