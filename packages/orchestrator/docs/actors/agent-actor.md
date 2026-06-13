# AgentActor

One AgentActor is spawned per registered agent at `agent:<agentId>`.

Private state:

- agent spec
- status: `idle | running | waiting | failed | stopped`
- current task ID
- transcript/messages
- current task plan
- engine continuation state
- pending resource requests
- cancellation state

Messages:

```ts
type AgentMsg =
  | { type: "dispatch"; task: AgentTask }
  | { type: "cancel"; taskId: string; reason?: string }
  | { type: "wake"; reason: WakeReason };
```

## Engine Loop

```text
dispatch(task)
  mark running
  await emit task_started

  while task is active:
    call engine.step(input)
    stream model deltas as task_delta

    if step completed:
      await emit task_completed
      reply terminal result
      mark idle
      return

    if step needs resources:
      resolve resources through actor asks
      store resource results for next engine input
      continue

    if max steps reached:
      await emit task_failed or task_cancelled
      reply terminal result/error
      mark idle
      return
```

The AgentActor owns the transcript for its own agent during a run. Host may
persist projections, but should not be required for the actor to continue.

## Engine Integration

`AgentActor` receives `StatelessEngine` as an injected dependency. The engine
does not know actors. It only receives an input snapshot and returns a step
result or stream of step events.

```ts
interface AgentActorDeps {
  engine: StatelessEngine;
  emit(event: OrchestratorEvent): Promise<void>;
  buildEngineInput(task: AgentTask, state: AgentRuntimeState): EngineInput;
  maxProviderRetries?: number;
}
```

The AgentActor owns the state needed to call the stateless engine repeatedly:

- transcript/messages
- system prompt for this agent
- model/tool configuration
- engine continuation/checkpoint returned by the previous step
- pending resource results
- total step count

Engine events map to Orchestrator events:

| Engine output | AgentActor action |
| --- | --- |
| assistant text delta | append transcript delta, `await emit(task_delta)` |
| thinking delta | append hidden/thinking delta, `await emit(task_delta)` |
| tool/resource request | pause engine loop, resolve through actor asks |
| assistant message complete | append message to transcript |
| step completed | emit `task_completed`, reply to caller |
| provider error | retry if transient, otherwise fail task |

The engine should not execute Orchestrator resources directly. Resource
resolution is an AgentActor/ToolActor concern.

## Resource Resolution

When the engine asks for resources, AgentActor pauses the model loop and uses
actor messages:

```text
approval/user input -> ask tool:registry execute host/orchestrator provider tool
tool call           -> ask tool:registry execute
subagent call       -> ask agent:<id>
```

Resource results are fed into the next engine step as structured results. A
resource failure should usually become a tool/resource result visible to the
model, not an immediate task failure, unless policy marks it fatal.

## Retry And Error Recovery

Do not put business retry in the actor kernel. AgentActor owns task-level retry
and recovery.

| Error type | Recovery |
| --- | --- |
| transient provider/network error before state changes | retry same engine step with backoff |
| provider error after partial stream | fail task unless engine can provide a safe retry checkpoint |
| tool execution error | pass structured error result back to engine by default |
| approval decline | pass declined result back to engine |
| subagent failure | policy: structured subagent error or parent task failure |
| actor/kernel error while resolving resource | fail current task |
| cancellation | emit `task_cancelled`, reject/reply cancellation, cleanup |

Retry safety rule:

```text
retry only if replaying the step cannot duplicate external side effects
```

That usually means retry provider/model calls before tool execution. Do not
retry a completed tool call by re-running the whole step unless the engine has a
checkpoint proving the call was not executed twice.

## Terminal Cleanup

Every terminal path must do the same cleanup:

```text
terminal path
  notify ToolActor task_finished for provider-owned task resources/subtasks
  clear current task state
  emit exactly one terminal task event
  reply or reject original dispatch ask
```

Terminal events are mutually exclusive:

- `task_completed`
- `task_failed`
- `task_cancelled`

## Subagents

Subagent delegation has two modes.

Blocking call:

```text
parent model calls delegate_to_agent(mode: "call")
  ToolActor routes to OrchToolProvider (in host-runtime)
  OrchToolProvider calls orchestrator.dispatchDetached() then joinTask()
  parent waits
  reviewer AgentActor processes task
  parent continues with result
```

Detached work with later join:

```text
parent model calls delegate_to_agent(mode: "detach")
  ToolActor routes to OrchToolProvider
  OrchToolProvider calls orchestrator.dispatchDetached(), returns taskId handle
  parent model receives SubtaskHandle and continues local work

reviewer finishes early
  result stored by orchestrator.detachedTasks
  parent AgentActor is not interrupted

parent model later calls join_subtask(handle)
  provider returns stored result or awaits pending promise
```

For this to work, AgentActor must support dispatch messages with correlation IDs
and reply with the terminal task result.

First design target: one task at a time per AgentActor. Per-task child actors
can be added later if parallel tasks within a single agent become necessary.
