# ToolActor

`ToolActor` is a pure per-call tool executor. Unlike the earlier design, tool discovery and registration are handled by the synchronous `ToolRegistry` container, not by the actor itself. One `ToolActor` instance is spawned dynamically per step (or per tool call) to execute tool actions.

Responsibilities:

- Receive one pre-resolved execute message with routing information
- Request Host/user approval for sensitive tools when required via `ApprovalGateway`
- Call the selected provider's `execute()` method
- Emit tool lifecycle events (`tool_started`, `tool_finished`, `approval_resolved`) through `await emit(...)`
- Return structured `ToolExecResult` to the calling AgentActor

ToolActor does not:

- Run model steps
- Perform tool discovery (which is delegated to `ToolRegistry.discoverTools()`)
- Decide parent task completion
- Keep a persistent registry of ToolProviders or ToolSets (the registry lives in the `ToolRegistry` dependency)

## Messages

```ts
export type ToolMsg =
  | {
      type: "execute";
      call: ToolCall;
      context: ToolExecutionContext;
      /** Pre-resolved route from ToolRegistry.discoverTools(). */
      route: CatalogRoute;
    }
  | { type: "cancel"; callId: string; reason?: string }
  | { type: "task_finished"; agentId: string; taskId: string };
```

## Execution

```text
execute(call)
  await emit tool_started

  if policy/definition requires approval:
    decision = await approvalGateway.requestToolApproval(request)
    await emit approval_resolved (decision)
    if declined:
      await emit tool_finished declined
      reply declined result
      return

  try:
    provider = resolve provider from route.providerId
    result = await provider.execute(call, context)
    await emit tool_finished result
    reply result
  catch error:
    await emit tool_finished error
    reply structured tool error
```

Errors from tool execution should normally become structured `ToolExecResult`s. Returning structured errors lets the model recover.

## Policy

Tool policy comes from ToolSet definitions and tool definitions, not from ad hoc AgentActor logic.

```ts
export type ToolSensitivity = "safe" | "sensitive" | "dangerous" | "dynamic";
export type ToolApprovalPolicy = "never" | "on_sensitive" | "always";
export type ToolExecutionMode = "parallel" | "sequential";
export type ToolFailureMode = "return_error" | "fail_task";

export interface ToolPolicy {
  sensitivity?: ToolSensitivity;
  approval?: ToolApprovalPolicy;
  timeoutMs?: number;
  executionMode?: ToolExecutionMode;
  failureMode?: ToolFailureMode;
}
```

## Approval

Approval is about user/Host permission. It pauses the caller until the Host-provided ApprovalGateway receives a decision.

Examples:
- Running a risky command
- Accessing a local port
- Opening an external app or URL
- Asking the user to confirm a plan

These cases generally need pause/resume only:

```text
AgentActor awaits ToolActor
  ToolActor awaits ApprovalGateway.requestToolApproval
    Host/TUI asks user
  ToolActor resumes after approval decision
AgentActor resumes after tool result
```

This is separate from model-requested interaction. If the model should be able to ask the user or request an explicit confirmation, Host exposes that as a normal provider-backed tool such as `ask_user` or `request_approval`, and the agent must have that tool through its ToolSets.

## Result Shape

Every tool result returned to AgentActor is a structured `ToolExecResult`:

```ts
export interface ToolExecResult {
  ok: boolean;
  value?: unknown;
  error?: { code: string; message: string; retryable?: boolean };
}
```

Do not throw ordinary tool failures across the model loop unless the failure is runtime-fatal. Returning structured errors lets the model recover.
