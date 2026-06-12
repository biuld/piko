# Host Integration

Host integrates Orchestrator as a runtime, not as internal mutable state.

## Host Responsibilities

- create Orchestrator with engine and toolset adapters
- register default agents and toolsets
- call `dispatch()` or `run()`
- implement `HostToolProvider` for user-facing tools such as `ask_user` and
  approval prompts
- subscribe to Orchestrator events
- map events to lifecycle/session/TUI state
- persist final messages and useful event traces if desired

## Host Must Not

- mutate actor private state
- execute subagents itself
- bypass provider boundaries or reach into provider internals
- own agent transcripts during a run
- call the Engine directly when Orchestrator mode is enabled
- read kernel mailboxes or actor cells

## Public Calls

Facade calls cross actor boundaries, so they are promise-based:

```ts
orchestrator.run(prompt, options)
  -> ask orchestrator:main run

orchestrator.dispatch(task)
  -> ask orchestrator:main dispatch

orchestrator.cancelTask(taskId)
  -> ask orchestrator:main cancel_task

orchestrator.snapshot()
  -> ask orchestrator:state snapshot
```

The Host should prefer event subscription for streaming UI updates. Snapshot is
for point-in-time inspection and graph rendering.

## Host Approval / Ask User Flow

```text
ToolActor
  awaits HostToolProvider.askApproval/request_user_input
    Host/TUI renders prompt
    user responds
  HostToolProvider resolves promise
  ToolActor resumes and returns structured tool result
```

Host/TUI interaction is a provider capability, not a core actor. ToolActor may
still emit `approval_requested` / `approval_resolved` events for observability
before and after awaiting the HostProvider promise.

## Session Persistence

Orchestrator does not write sessions. Host decides what to persist:

- user-visible assistant/user messages
- tool lifecycle summaries
- approval decisions
- optional Orchestrator event trace for debugging

The event stream should be stable enough to support future JSON/RPC streaming,
but durable replay is not a design requirement.
