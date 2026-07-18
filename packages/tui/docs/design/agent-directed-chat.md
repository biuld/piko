# Agent-Directed Chat Design

## Status

Implemented.

## Scope

The Editor submits text to the concrete AgentInstance selected in
`AgentPanelState`. Root and child AgentInstances use the same wire command,
hostd Turn lifecycle, and Agent run API.

This feature changes command routing and lifecycle projection. It does not add
a Panel, Slot, overlay, focus target, setting, or key binding.

## User-visible contract

- Timeline shows the selected AgentInstance transcript.
- A newly created Session starts with its root AgentInstance selected, so the
  first Editor submission has a concrete target without manual selection.
- Enter captures the selected `agent_instance_id` and sends the text to that
  target.
- Every accepted submission creates one hostd Turn.
- One AgentInstance has at most one running Turn. Later submissions to that
  AgentInstance are queued in submission order.
- Different AgentInstances in the same Session may run Turns concurrently.
- Streaming output and committed messages remain scoped to the target
  AgentInstance.
- Switching Agent selection after submission does not retarget the accepted
  Turn.
- Esc cancels the active Turn for the AgentInstance currently shown.

## Wire contract

The TUI sends one command for every Editor submission:

```rust
Command::ChatSubmit {
    command_id,
    session_id,
    target_agent_instance_id,
    text,
}
```

The selected AgentInstance is explicit. hostd does not infer the target from
root identity, AgentSpec ID, display name, or current server-side selection.

All ChatSubmit-originated lifecycle events use `TurnEvent`:

```rust
TurnEvent::Queued {
    session_id,
    turn_id,
    agent_instance_id,
    timestamp,
}

TurnEvent::Started {
    session_id,
    turn_id,
    agent_instance_id,
    timestamp,
}
```

`Completed`, `Failed`, and `Cancelled` carry the same
`agent_instance_id`. `SessionSnapshot.active_turns` contains target-aware
`TurnSnapshot` values.

`ServerMessage::AgentRunLifecycle` is not used for ChatSubmit-originated Agent
runs. `AgentChanged` remains authoritative for Agent lifecycle and activity.

## TUI state

`SessionUiState.active_turns` maps `agent_instance_id` to `turn_id`.
`AppState::active_turn_id` resolves the entry for
`AgentPanelState.active_agent_instance_id`.

This makes rendering and cancellation selection-aware without conflating
concurrent Turns in the same Session. `AgentPanelState` derives running state
for the selected Agent from the same mapping.

`SessionReconciled` replaces `active_turns` from the authoritative
`SessionSnapshot`. Incremental `TurnEvent` values update one target entry.

## hostd flow

`HostApp::apply_chat_submit` validates the Session and target, then calls the
target-neutral `HostApp::submit_chat` method.

`HostState::start_turn` creates the target-aware Turn. It does not schedule the
target AgentInstance. Every ChatSubmit-originated request uses
`AgentInputDelivery::FollowUp`; `AgentActor` either starts it or persists a
`DurableAgentInput` through `AgentDurableCommand::InputQueued`.

hostd invokes one runner interface:

```rust
AgentRunRunner::run_agent(AgentRunInput) -> AgentRunHandle
```

`AgentRunHandle` includes the `AgentInputReceipt` that tells hostd whether the
input started or was queued, a `started` receiver that yields its
`SessionSubscription`, and its completion receiver. orchd-api calls its
lower-level return value `AgentRunAcceptance`; it does not define a second
`AgentRunHandle`. `AgentRunInput` always carries `session_id`, `operation_id`,
`agent_instance_id`, and `source_turn_id`. For a Turn-originated run,
`operation_id` and `source_turn_id` correlate to the `turn_id`.

`AgentActor::advance_next_follow_up` starts queued input after the prior run is
terminal. `AgentRunCompletion` returns an `AgentOperationAddress`, durable
`AgentRunReport`, and observation barrier. hostd validates the addressed
AgentInstance before applying the terminal result to the Turn.

## Transcript and observation

orchd commits each message to the target AgentInstance shard. hostd projects
reliable commits and realtime deltas with their `agent_instance_id`; the TUI
only applies them to the matching Agent view.

The completion channel determines the business result. Observation supplies
projection and must reach `AgentRunCompletion.observation_barrier` before
hostd emits the terminal `TurnEvent`.

## Cancellation and compaction

`Command::TurnCancel` addresses a Turn by `session_id + turn_id`. hostd resolves
the immutable target and calls `AgentRunRunner::cancel_agent_run` with the full
`AgentOperationAddress`. Cancelling a queued Turn calls
`AgentRuntimeApi::cancel_agent_input`, durably commits
`AgentDurableCommand::QueuedInputCancelled`, and removes the matching
`DurableAgentInput` without starting an Agent run.

`Command::SessionCompact` includes an explicit `agent_instance_id`. Current
`SessionTreeEntry` compaction is applied only to the root AgentInstance; hostd
does not accidentally compact root state when a child is selected.

## Error handling

- An unknown, closed, or unavailable target rejects the command.
- A busy target queues the Turn instead of redirecting or returning a
  root-specific error.
- A mismatched `AgentRunReport.agent_instance_id` fails the operation and does
  not complete the Turn successfully.
- Session reconciliation replaces stale local Turn state.

## Non-goals

- Sending one Editor submission to multiple AgentInstances.
- Reopening a closed AgentInstance automatically.
- Changing Agent hierarchy or authorization policy.
- Adding new TUI layout or focus behavior.
