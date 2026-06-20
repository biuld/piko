# Host Integration

Host integrates Orchestrator as a runtime, not as internal mutable state.

## Host Responsibilities

- create Orchestrator with ModelStepExecutor
- register default agents and toolsets
- call `dispatch()` or `run()`
- implement `HostToolProvider` for model-visible user-facing tools such as
  `ask_user` and explicit approval-request tools
- provide `ApprovalGateway` for tool approval
- subscribe to Orchestrator events
- map events to lifecycle/session/TUI state
- persist final messages and useful event traces if desired

## Host Must Not

- mutate AgentActor internal state
- execute subagents itself
- bypass provider boundaries or reach into provider internals
- own agent transcripts during a run
- call the ModelStepExecutor directly when Orchestrator mode is enabled
- read kernel mailboxes or actor cells

## Public Calls

Facade calls are direct method calls on the `Orchestrator` object — there is no
intermediate `orchestrator:main` actor. The facade delegates to helper modules:

```ts
orchestrator.run(prompt, options)
  → task.run(ctx, prompt, opts)
  → emit orchestrator_started
  → createRun() → spawn AgentActor → await resultPromise

orchestrator.dispatch(task)
  → task.dispatch(ctx, task)
  → createRun() → spawn AgentActor → ask dispatch → return taskId immediately
  (resultPromise is fire-and-forget; errors are silently caught)

orchestrator.dispatchDetached(task)
  → task.dispatchDetached(ctx, task)
  → createRun(retainForJoin: true) → spawn AgentActor

orchestrator.cancelTask(taskId)
  → task.cancelTask(ctx, taskId)
  → run.status = "cancelling" → ask AgentActor cancel

orchestrator.delegateToAgent(task)
  → task.delegateToAgent(ctx, task)
  → createRun() → await resultPromise

orchestrator.delegateDetached(task)
  → task.delegateDetached(ctx, task)
  → createRun(retainForJoin: true) → return taskId

orchestrator.joinTask(taskId)
  → task.joinTask(ctx, taskId)
  → await run.resultPromise

orchestrator.snapshot()
  → state.snapshot(ctx) → ctx.eventStore.snapshot()   // synchronous

orchestrator.subscribe(listener)
  → state.subscribe(ctx) → ctx.eventStore.subscribe(listener)

orchestrator.getGraph()
  → state.getGraph(ctx) → ctx.eventStore.graph()

orchestrator.updatePlan(agentId, taskId, plan)
  → state.updatePlan(ctx, agentId, taskId, plan)
  → emit plan_updated
```

The Host should prefer event subscription for streaming UI updates. Snapshot is
for point-in-time inspection and graph rendering.

## Host Approval / Ask User Flow

Tool approval uses the `ApprovalGateway` interface, provided by Host:

```text
ToolRegistryImpl.executeTool()
  checks effective approval policy on the tool
  if approval needed ("always" or "on_request"):
    emits tool_started
    awaits ApprovalGateway.requestToolApproval(request, signal)
      Host/TUI renders prompt
      user responds
    ApprovalGateway resolves with "accept" or "decline"
    if declined → emits approval_resolved(decline), returns error result
    if accepted → emits approval_resolved(accept)
  calls provider.execute(call, context, signal)
  emits tool_finished
  returns ToolExecResult
```

Model-requested user interaction remains a provider capability. For example,
Host may expose `ask_user` or `request_approval` through `HostToolProvider` so
the model can initiate those requests as ordinary scoped tools. `ToolRegistryImpl`
applies lifecycle events, policy, cancellation, and structured results around
that provider call.

## Session Persistence

Orchestrator does not write sessions. Host decides what to persist:

- user-visible assistant/user messages
- tool lifecycle summaries
- approval decisions
- optional Orchestrator event trace for debugging

The event stream should be stable enough to support future JSON/RPC streaming,
but durable replay is not a design requirement.
