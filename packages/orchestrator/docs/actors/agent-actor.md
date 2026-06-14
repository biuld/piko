# AgentActor

One AgentActor is spawned per registered agent at `agent:<agentId>`.

Private state:

- agent spec
- status: `idle | running | failed | stopped`
- current task ID
- transcript/messages
- current task plan
- model continuation state (`engineState`)
- cancellation state

Messages:

```ts
type AgentMsg =
  | { type: "dispatch"; task: AgentTask }
  | { type: "cancel"; taskId: string; reason?: string }
  | { type: "wake"; reason: { type: string; taskId?: string; approvalId?: string } }
  | {
      type: "set_model_config";
      config: {
        model?: { id: string; name?: string; provider?: string };
        provider?: Record<string, unknown>;
        settings?: { maxSteps?: number; allowToolCalls?: boolean; allowApprovals?: boolean };
      };
    };
```

## Model Step Loop

```text
dispatch(task)
  mark running
  await emit task_started

  while task is active:
    call executor.executeStep(input)
    stream model deltas as task_delta

    if step completed (status completed / error / aborted):
      await emit task_completed or task_failed
      reply terminal result
      mark idle
      return

    if step needs tools to execute:
      execute tool calls (parallel or sequential)
      append tool results to transcript
      continue

    if max steps reached:
      await emit task_failed
      reply terminal error
      mark idle
      return
```

The AgentActor owns the transcript for its own agent during a run. Host may
persist projections, but should not be required for the actor to continue.

## Model Step Executor Integration

`AgentActor` receives `ModelStepExecutor` as an injected dependency. The executor
does not know actors. It only receives an input snapshot and returns a step
result or stream of step events.

```ts
export interface AgentActorDeps {
  modelExecutor: ModelStepExecutor;
  emit: (event: OrchestratorEvent) => Promise<void>;
  maxSteps?: number;
  modelConfig?: {
    model: import("piko-orchestrator-protocol").Model<string>;
    provider: ModelProviderConfig;
    settings: ModelRunSettings;
  };
  actorSystem?: import("../../kernel/actor-system.js").ActorSystem;
  toolRegistry: ToolRegistry;
}
```

The AgentActor owns the state needed to call the ModelStepExecutor repeatedly:

- transcript/messages
- system prompt for this agent
- model/tool configuration
- model continuation state (`engineState`)
- total step count

ModelStepEvents map to Orchestrator events:

| ModelStepEvent | AgentActor action |
| --- | --- |
| `message_delta` | `await emit(task_delta)` with text delta |
| `thinking_delta` | `await emit(task_delta)` with thinking delta |
| `message_end` | append message to transcript |
| step completed (status) | emit terminal event if completed/error/aborted |

The ModelStepExecutor does not execute tools directly. Tool execution is handled by spawning per-call/per-step ToolActors.

## Resource Resolution

When the model asks to execute tools, AgentActor pauses its loop and spawns ToolActor(s):

```text
tool call -> spawn ToolActor -> ask ToolActor execute
```

Tool results are appended to the transcript and fed into the next step. A tool execution error becomes a tool result visible to the model, rather than an immediate task failure (unless the failure mode is set to fail task).

## Retry And Error Recovery

Do not put business retry in the actor kernel. AgentActor owns task-level retry and recovery.

| Error type | Recovery |
| --- | --- |
| transient provider/network error | retry same step with backoff |
| tool execution error | pass structured error result back to model by default |
| approval decline | pass declined result back to model |
| subagent failure | policy: structured subagent error or parent task failure |
| actor/kernel error | fail current task |
| cancellation | emit `task_cancelled`, reject/reply cancellation, cleanup |

Retry safety rule:

```text
retry only if replaying the step cannot duplicate external side effects
```

That usually means retry model calls before tool execution.

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
  ToolActor routes to OrchToolProvider (in orchestrator/src/tools/orch-provider.ts)
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

For this to work, AgentActor must support dispatch messages with correlation IDs and reply with the terminal task result.

First design target: one task at a time per AgentActor. Per-task child actors can be added later if parallel tasks within a single agent become necessary.
