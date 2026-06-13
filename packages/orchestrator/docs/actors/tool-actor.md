# ToolActor

`ToolActor` owns discovery, routing, and execution coordination for tools. It
may be one global actor or split by provider/namespace later.

Responsibilities:

- register/unregister `ToolProvider`s
- register/unregister `ToolSet`s
- discover all available tools from providers
- build the provider-visible tool catalog for AgentActor
- receive tool execution requests from AgentActor
- resolve static ToolSet policy
- request Host/user approval for sensitive tools when required
- call the selected provider's `execute()`
- emit tool lifecycle events through `await emit(...)`
- return structured tool results to AgentActor

ToolActor should not:

- run model steps
- mutate agent transcripts directly
- decide parent task completion

ToolActor does not know how a provider implements file writes or write
serialization. If a concrete write-capable tool needs an internal file mutex,
that belongs inside the provider/tool implementation.

## Discovery

Tool discovery is explicit and scoped by agent/task context.

```ts
interface ToolDiscoveryContext {
  agentId: string;
  taskId?: string;
  toolSetIds: string[];
  activeToolNames?: string[];
}
```

`ToolActor` asks each provider to discover tools, then merges and filters them:

```text
discover_tools(context)
  for provider in providers:
    tools += await provider.discover(context)
  load ToolSets listed in context.toolSetIds
  apply ToolSet selection/aliases/policy
  apply active tool restrictions
  validate duplicate names/routes
  return ToolCatalog
```

Tool names must be globally unique within the catalog visible to one engine
step. If two providers expose the same name, discovery should fail unless a
namespace rule disambiguates them.

AgentActor uses discovery before engine calls:

```text
AgentActor
  catalog = await ask tool:registry discover_tools
  EngineInput.tools = catalog.providerVisibleDefinitions
  engine.step(input)
```

## Messages

```ts
type ToolMsg =
  | { type: "register_provider"; provider: ToolProvider }
  | { type: "unregister_provider"; providerId: string }
  | { type: "register_tool_set"; toolSet: ToolSet }
  | { type: "unregister_tool_set"; toolSetId: string }
  | { type: "discover_tools"; context: ToolDiscoveryContext }
  | { type: "execute"; call: ToolCall; context: ToolExecutionContext }
  | { type: "cancel"; callId: string; reason?: string }
  | { type: "task_finished"; agentId: string; taskId: string };
```

## Execution

```text
execute(call)
  await emit tool_started

  if policy says call is sensitive and requires approval:
    decision = await hostProvider.askApproval(request)
    if declined:
      await emit tool_finished declined
      reply declined result
      return

  try:
    provider = resolve provider route for call.name
    result = await provider.execute(call, context)
    await emit tool_finished result
    reply result
  catch error:
    await emit tool_finished error
    reply structured tool error
```

Errors from tool execution should normally become structured tool results. They
should fail the task only when policy marks the tool failure as fatal.

## Policy

Tool policy comes from ToolSet definitions, not from ad hoc AgentActor logic.
ToolActor's policy model is about sensitivity and approval. It does not model
file locking or workspace serialization.

```ts
interface ToolPolicy {
  sensitivity?: "safe" | "sensitive" | "dangerous" | "dynamic";
  approval?: "never" | "on_sensitive" | "always";
  timeoutMs?: number;
  executionMode?: "parallel" | "sequential";
  failureMode?: "return_error" | "fail_task";
}
```

Policy interpretation:

```text
sensitivity: "safe"
  -> normally no approval

sensitivity: "sensitive"
  -> approval when approval = "on_sensitive"

sensitivity: "dangerous"
  -> approval unless approval = "never" by explicit trusted policy

sensitivity: "dynamic"
  -> provider classifies the concrete call from arguments before approval check
```

## Approval

Approval is about user/Host permission. It pauses the caller until
the Host-provided ApprovalGateway receives a decision.

Examples:

- running a risky command
- accessing a local port
- opening an external app or URL
- asking the user to confirm a plan

These cases generally need pause/resume only:

```text
AgentActor awaits ToolActor
  ToolActor awaits ApprovalGateway.requestToolApproval
    Host/TUI asks user
  ToolActor resumes after approval decision
AgentActor resumes after tool result
```

This is separate from model-requested interaction. If the model should be able
to ask the user or request an explicit confirmation, Host exposes that as a
normal provider-backed tool such as `ask_user` or `request_approval`, and the
agent must have that tool through its ToolSets.

File write serialization is not part of this policy. For example, `apply_patch`
may internally serialize writes inside `EngineToolProvider`, but ToolActor only
sees a provider execution request and result.

## Result Shape

Every tool result returned to Engine should be structured.

```ts
type OrchestratorToolResult =
  | { ok: true; value: unknown }
  | { ok: false; error: { code: string; message: string; retryable?: boolean } };
```

Do not throw ordinary tool failures across the engine loop unless the failure is
runtime-fatal. Returning structured errors lets the model recover.
