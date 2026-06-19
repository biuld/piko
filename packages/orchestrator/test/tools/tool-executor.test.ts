// ---- ToolRegistry.executeTool tests — stateless tool execution ----

import { describe, expect, it } from "bun:test";
import type {
  ApprovalGateway,
  ToolApprovalDecision,
  ToolApprovalRequest,
  ToolDef,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../../src/actors/state/index.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";
import type { CatalogRoute } from "../../src/tools/tool-registry.js";
import { ToolRegistryImpl } from "../../src/tools/tool-registry.js";
import { createMockToolProvider } from "../helpers/index.js";

// ---- Helpers ----

function makeRoute(providerId: string, providerToolName: string, toolDef: ToolDef): CatalogRoute {
  return { providerId, providerToolName, toolDef };
}

function makeToolDef(name: string, opts?: Partial<ToolDef>): ToolDef {
  return {
    name,
    description: `Tool: ${name}`,
    inputSchema: { type: "object", properties: {} },
    executor: { kind: "native", target: name },
    ...opts,
  };
}

function createTestToolExecutor(options?: {
  providers?: Map<string, ToolProvider>;
  approvalGateway?: ApprovalGateway;
}) {
  const system = new ActorSystem();
  const emitted: OrchestratorEvent[] = [];

  const emit = async (event: OrchestratorEvent) => {
    emitted.push(event);
  };

  const registry = new ToolRegistryImpl(emit);
  if (options?.providers) {
    for (const [_id, p] of options.providers) {
      registry.registerProvider(p);
    }
  }
  if (options?.approvalGateway) {
    registry.setApprovalGateway(options.approvalGateway);
  }

  return {
    system,
    emitted,
    execute: (
      call: { id: string; name: string; arguments: Record<string, unknown> },
      context: ToolExecutionContext,
      route: CatalogRoute,
    ) => registry.executeTool({ type: "toolCall", ...call }, context, route),
  };
}

// ---- Tests ----

describe("ToolRegistry", () => {
  // ---- Execution ----

  it("executes a tool through the provider", async () => {
    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
        executeResult: { ok: true, value: "file.txt\n" },
      }),
    );

    const { execute, emitted } = createTestToolExecutor({ providers });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: { command: "ls" } },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "bash", makeToolDef("bash")),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toBe("file.txt\n");
    expect(emitted.some((e) => e.type === "tool_started")).toBe(true);
    expect(emitted.some((e) => e.type === "tool_finished")).toBe(true);
  });

  it("returns not_found for missing provider", async () => {
    const providers = new Map<string, ToolProvider>();

    const { execute } = createTestToolExecutor({ providers });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: {} },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("nonexistent", "bash", makeToolDef("bash")),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("not_found");
  });

  it("returns structured error when provider execution throws", async () => {
    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
        executeFn: async () => {
          throw new Error("provider crash");
        },
      }),
    );

    const { execute, emitted } = createTestToolExecutor({ providers });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: { command: "rm -rf /" } },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "bash", makeToolDef("bash")),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("execution_error");
    expect(emitted.some((e) => e.type === "tool_finished")).toBe(true);
  });

  // ---- Approval ----

  it("approval always → calls gateway, continues on accept", async () => {
    const gateway: ApprovalGateway = {
      requestToolApproval: async (_req: ToolApprovalRequest): Promise<ToolApprovalDecision> =>
        "accept",
    };

    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
        executeResult: { ok: true, value: "ok" },
      }),
    );

    const { execute, emitted } = createTestToolExecutor({ providers, approvalGateway: gateway });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: { command: "ls" } },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "bash", makeToolDef("bash", { approval: "always" })),
    );

    expect(result.ok).toBe(true);
    expect(
      emitted.some(
        (e) => e.type === "approval_resolved" && (e as { decision: string }).decision === "accept",
      ),
    ).toBe(true);
  });

  it("approval always → calls gateway, declines and returns error", async () => {
    const gateway: ApprovalGateway = {
      requestToolApproval: async (_req: ToolApprovalRequest): Promise<ToolApprovalDecision> =>
        "decline",
    };

    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
      }),
    );

    const { execute, emitted } = createTestToolExecutor({ providers, approvalGateway: gateway });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: {} },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "bash", makeToolDef("bash", { approval: "always" })),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("declined");
    expect(
      emitted.some(
        (e) => e.type === "approval_resolved" && (e as { decision: string }).decision === "decline",
      ),
    ).toBe(true);
  });

  it("approval never → skips gateway entirely", async () => {
    let gatewayCalled = false;
    const gateway: ApprovalGateway = {
      requestToolApproval: async (_req: ToolApprovalRequest): Promise<ToolApprovalDecision> => {
        gatewayCalled = true;
        return "accept";
      },
    };

    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
        executeResult: { ok: true, value: "ok" },
      }),
    );

    const { execute } = createTestToolExecutor({ providers, approvalGateway: gateway });

    const result = await execute(
      { id: "call-1", name: "bash", arguments: {} },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "bash", makeToolDef("bash", { approval: "never" })),
    );

    expect(result.ok).toBe(true);
    expect(gatewayCalled).toBe(false);
  });

  // ---- Lifecycle events ----

  it("emits tool_started with correct metadata", async () => {
    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash")],
        executeResult: { ok: true, value: "ok" },
      }),
    );

    const { execute, emitted } = createTestToolExecutor({ providers });

    await execute(
      { id: "call-abc", name: "shell", arguments: { command: "ls" } },
      { agentId: "agent-1", taskId: "task-42", toolSetIds: [] },
      makeRoute("engine", "run_shell_command", makeToolDef("shell")),
    );

    const startEvent = emitted.find(
      (e): e is Extract<OrchestratorEvent, { type: "tool_started" }> => e.type === "tool_started",
    );
    expect(startEvent).toBeDefined();
    expect(startEvent!.callId).toBe("call-abc");
    expect(startEvent!.name).toBe("shell");
    expect(startEvent!.agentId).toBe("agent-1");
    expect(startEvent!.taskId).toBe("task-42");
  });

  // ---- Alias (providerToolName ≠ publicName) ----

  it("executes tool via alias name", async () => {
    const providers = new Map<string, ToolProvider>();
    providers.set(
      "engine",
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("run_shell_command")],
        executeResult: { ok: true, value: "output" },
      }),
    );

    const { execute } = createTestToolExecutor({ providers });

    const result = await execute(
      { id: "call-1", name: "shell", arguments: { command: "ls" } },
      { agentId: "agent-1", taskId: "task-1", toolSetIds: [] },
      makeRoute("engine", "run_shell_command", makeToolDef("shell")),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toBe("output");
  });
});
