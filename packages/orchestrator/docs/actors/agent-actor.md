# AgentActor

One AgentActor is spawned **per task** (not per registered agent). Its actor ID
is `agent:<agentId>:task:<taskId>`. The actor is stopped automatically by calling
`ctx.stop(ctx.self.id)` after it finalizes (emits a terminal event and replies
to the pending dispatch `ask`).

Private state (owned by `AgentActorInstance`):

- `spec` — agent spec (id, system prompt, toolSetIds, etc.)
- `status` — `"idle" | "running" | "cancelling"` (elided: `"failed"`, `"stopped"` are defined in the type but not actively used)
- `currentTaskId` — task being executed
- `currentRunToken` — monotonic token to match worker callbacks
- `pendingReply` — envelope to reply to when the task finishes
- `abortController` — used to signal the worker to stop
- `terminalCommitted` — guards against emitting more than one terminal event

Worker state (ephemeral, not shared across tasks):

- `transcript: Message[]` — accumulated message transcript
- `stepCount: number` — current step counter
- `engineState?: unknown` — model continuation state carried between steps

Messages:

```ts
type AgentMsg =
  | { type: "dispatch"; task: AgentTask }
  | { type: "cancel"; taskId: string; reason?: string }
  | { type: "wake"; reason: { type: string; taskId?: string; approvalId?: string } }
  | { type: "set_model_config"; config: { model?: {...}; provider?: {...}; settings?: {...} } }
  | { type: "runner_finished"; taskId: string; token: number; result: any }
  | { type: "runner_failed"; taskId: string; token: number; error: string };
```

`runner_finished` and `runner_failed` are sent by the async worker back to the
actor's own mailbox to serialize the result delivery.

## Run Flow

```text
dispatch(task) received
  reject if already running (status === "running")
  mark status = "running", record pendingReply, assign runToken
  create AbortController
  await emit task_started
  startAgentRun() — fires the async worker and returns immediately

[async worker runs concurrently via Promise chain]
  startAgentRun:
    build initialTranscript: task.history + { role: "user", content: task.prompt }
    create AgentWorkerState { transcript, stepCount: 0 }
    call runEngineLoop(state, workerState, deps, ctx, task, signal)
    on resolve → send runner_finished to own mailbox
    on reject  → send runner_failed to own mailbox

  runEngineLoop (infinite loop until terminal):
    1. Discover tools: toolRegistry.discoverTools(agentId, toolSetIds)
    2. Run model step: runModelStep() → streams ModelStepEvents, emits deltas
    3. Process outcome: processStepOutcome() → tool execution or terminal

runner_finished received
  verify token + taskId match (stale callbacks are dropped)
  finalize(completed, result)

finalize()
  guard terminalCommitted (idempotent)
  emit task_transcript_committed + terminal event (task_completed / task_failed / task_cancelled)
  reply pendingReply with result
  ctx.stop(self) — actor is stopped and removed from ActorSystem
```

## Cancel Flow

```text
cancel(taskId) received
  if status === "running" and taskId matches:
    status = "cancelling"
    abortController.abort()
    reply immediately (cancel is acknowledged, not waited for)

[worker detects abort signal and returns aborted result]
  runner_failed received with aborted error OR
  runner_finished received with finalStatus === "aborted"

finalize(cancelled) runs
  emit task_cancelled
  reply original dispatch ask with { finalStatus: "aborted" }
  ctx.stop(self)
```

## Model Step Executor Integration

`AgentActor` receives `ModelStepExecutor` as an injected dependency. The executor
does not know actors. It only receives an input snapshot and returns a step
result via an `EventStream`.

```ts
export interface AgentActorDeps {
  modelExecutor: ModelStepExecutor;
  emit: (event: OrchestratorEvent) => Promise<void>;
  modelConfig?: { model: Model; provider: ModelProviderConfig; settings: ModelRunSettings };
  actorSystem?: ActorSystem;
  toolRegistry: ToolRegistry;   // DI container for tool discovery and execution (ToolRegistryImpl)
}
```

The AgentActor owns the state needed to call the ModelStepExecutor repeatedly:

- transcript/messages (in `AgentWorkerState`)
- model/tool configuration
- model continuation state (`engineState` on `workerState`)
- step count (`workerState.stepCount`)

ModelStepEvents map to Orchestrator events:

| ModelStepEvent | AgentActor action |
| --- | --- |
| `message_start` | `await emit(task_message_start)` |
| `message_delta` | `await emit(task_delta)` with delta `{ kind: "text", text }` |
| `thinking_delta` | `await emit(task_delta)` with delta `{ kind: "thinking", text }` |
| `message_update` | `await emit(task_message_update)` |
| `message_end` | convert to RuntimeMessage, `await emit(task_message_end)`, append to transcript |
| `step_start`, `step_end`, `error` | (informational, no emit) |
| stream result `completed`/`error`/`aborted` | `processStepOutcome()` determines terminal vs. continue |

The ModelStepExecutor does not execute tools. Tool execution is handled by
`ToolRegistry.executeTool()` called from `executeToolCalls()`.

## Resource Resolution

When the model asks to execute tools, the worker loop pauses and executes them:

```text
tool call → executeToolCalls() → ToolRegistry.executeTool() → ToolProvider.execute()
```

Tool results are appended to the transcript and fed into the next step.
A tool execution error becomes a tool result visible to the model (unless
`failureMode: "fail_task"` is set on the tool definition).

## Terminal Cleanup

Every terminal path runs `finalize()` exactly once:

```text
finalize()
  guard terminalCommitted = true (subsequent calls are no-ops)
  clear currentRunToken, currentTaskId, abortController
  emit task_transcript_committed (with full message list)
  emit exactly one terminal event:
    task_cancelled  (finalStatus === "aborted")
    task_failed     (finalStatus === "error")
    task_completed  (finalStatus === "completed")
  reply or reject original dispatch ask
  ctx.stop(self)
```

Terminal events are mutually exclusive:

- `task_completed`
- `task_failed`
- `task_cancelled`

## Retry And Error Recovery

| Error type | Recovery |
| --- | --- |
| transient provider/network error | retry same step with backoff |
| tool execution error | pass structured error result back to model by default |
| approval decline | pass declined result back to model |
| subagent failure | policy: structured subagent error or parent task failure |
| actor/kernel error | fail current task |
| cancellation | emit `task_cancelled`, reply with `finalStatus: "aborted"`, stop actor |

Retry safety rule:

```text
retry only if replaying the step cannot duplicate external side effects
```

That usually means retry model calls before tool execution.

## Subagents

Subagent delegation has two modes.

Blocking call:

```text
parent model calls delegate_to_agent(mode: "call")
  ToolRegistryImpl.executeTool() routes to OrchToolProvider (in orchestrator/src/tools/orch-provider.ts)
  OrchToolProvider calls orchestrator.delegateToAgent()
  orchestrator.delegateToAgent() creates a new AgentActor for the subagent task
  parent waits on result
  subagent AgentActor processes task, finalizes, stops itself
  parent continues with result
```

Detached work with later join:

```text
parent model calls delegate_to_agent(mode: "detach")
  ToolRegistryImpl.executeTool() routes to OrchToolProvider
  OrchToolProvider calls orchestrator.delegateDetached(), returns taskId handle
  parent model receives taskId and continues local work

parent model later calls join_subtask(taskId)
  OrchToolProvider calls orchestrator.joinTask(taskId)
  joinTask() awaits run.resultPromise from the RunHandle
  returns result when the subagent task finishes
```

AgentActor supports concurrent dispatch messages by checking status: if
`status === "running"` a second dispatch is rejected immediately. Per-task
child actors are used for isolation, not concurrency within one agent.
