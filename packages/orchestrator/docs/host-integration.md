# Host Integration

Host integrates Orchestrator as a runtime, not as internal mutable state.

## Host Responsibilities

- create Orchestrator with engine and toolset adapters
- register default agents and toolsets
- call `dispatch()` or `run()`
- implement `HostToolProvider` for model-visible user-facing tools such as
  `ask_user` and explicit approval-request tools
- provide `ApprovalGateway` for ToolActor policy approval
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

ToolActor policy approval uses an explicit Host API, not a ToolProvider route:

```text
ToolActor
  awaits ApprovalGateway.requestToolApproval
    Host/TUI renders prompt
    user responds
  ApprovalGateway resolves promise
  ToolActor resumes and returns structured tool result
```

Model-requested user interaction remains a provider capability. For example,
Host may expose `ask_user` or `request_approval` through `HostToolProvider` so
the model can initiate those requests as ordinary scoped tools. ToolActor still
applies lifecycle events, policy, cancellation, and structured results around
that provider call.

ToolActor may emit `approval_requested` / `approval_resolved` events for
observability before and after awaiting the ApprovalGateway promise.

## Session Persistence

Orchestrator does not write sessions. Host decides what to persist:

- user-visible assistant/user messages
- tool lifecycle summaries
- approval decisions
- optional Orchestrator event trace for debugging

The event stream should be stable enough to support future JSON/RPC streaming,
but durable replay is not a design requirement.
