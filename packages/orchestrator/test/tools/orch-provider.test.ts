// ---- OrchToolProvider unit tests (mock Orchestrator) ----
// Directly test every code branch, including catch paths that are
// hard/impossible to trigger through integration tests.

import { describe, expect, it } from "bun:test";
import type { ToolExecutionContext } from "piko-orchestrator-protocol";
import type { Orchestrator } from "../../src/orchestrator/index.js";
import { OrchToolProvider } from "../../src/tools/index.js";

// ---- Mock orchestrator builder ----

type MockOrch = {
  [K in keyof Orchestrator]: Orchestrator[K] extends (...args: infer A) => infer R
    ? (...args: A) => R
    : Orchestrator[K];
};

function createMockOrch(overrides: Partial<MockOrch> = {}): Orchestrator {
  return {
    registerAgent: () => {},
    unregisterAgent: () => {},
    registerToolSet: () => {},
    unregisterToolSet: () => {},
    setModelConfig: () => {},
    setApprovalGateway: () => {},
    registerProvider: () => {},
    dispatch: async () => "task-mock",
    dispatchDetached: async () => "task-mock-detached",
    delegateToAgent: async () => ({ taskId: "task-mock", result: { summary: "ok" } }),
    delegateDetached: async () => "task-mock-detached",
    joinTask: async () => ({ summary: "joined" }),
    run: async () => ({ messages: [], totalSteps: 0, status: "completed" }),
    subscribe: () => () => {},
    snapshot: () => ({
      runId: "mock-run",
      status: "idle",
      toolSets: {},
      agents: {
        implementer: { id: "implementer", spec: {} as never, status: "idle", transcript: [] },
        reviewer: { id: "reviewer", spec: {} as never, status: "idle", transcript: [] },
        busy: { id: "busy", spec: {} as never, status: "running", transcript: [] },
      },
      tasks: {},
    }),
    updatePlan: () => {},
    getGraph: async () => ({ nodes: [], edges: [] }),
    ...overrides,
  } as Orchestrator;
}

function makeCtx(overrides?: Partial<ToolExecutionContext>): ToolExecutionContext {
  return {
    agentId: "caller",
    taskId: "task-caller",
    toolSetIds: ["builtin"],
    ...overrides,
  };
}

// =========================================================================
// delegate_to_agent — catch paths
// =========================================================================

describe("OrchToolProvider unit — delegate_to_agent", () => {
  // ---- delegation_failed (detach) ----

  it("returns delegation_failed when delegateDetached throws", async () => {
    const mockOrch = createMockOrch({
      delegateDetached: async () => {
        throw new Error("mock delegateDetached failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "do work", mode: "detach" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("delegation_failed");
    expect(result.error?.message).toContain("mock delegateDetached failure");
  });

  // ---- delegation_failed (call) ----

  it("returns delegation_failed when delegateToAgent throws", async () => {
    const mockOrch = createMockOrch({
      delegateToAgent: async () => {
        throw new Error("mock delegateToAgent failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "do work", mode: "call" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("delegation_failed");
    expect(result.error?.message).toContain("mock delegateToAgent failure");
  });

  // ---- busy agent (previously skipped in integration test) ----

  it("returns agent_busy when target agent is running", async () => {
    const mockOrch = createMockOrch(); // busy agent has status "running" in snapshot
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "busy", prompt: "urgent work" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("agent_busy");
  });
});

// =========================================================================
// join_subtask — catch paths
// =========================================================================

describe("OrchToolProvider unit — join_subtask", () => {
  it("returns join_failed when joinTask throws", async () => {
    const mockOrch = createMockOrch({
      joinTask: async () => {
        throw new Error("mock joinTask failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "join_subtask",
        arguments: { taskId: "task-1" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("join_failed");
    expect(result.error?.message).toContain("mock joinTask failure");
  });
});

// =========================================================================
// get_orchestrator_state — catch paths
// =========================================================================

describe("OrchToolProvider unit — get_orchestrator_state", () => {
  it("returns state_read_failed when snapshot throws", async () => {
    const mockOrch = createMockOrch({
      snapshot: () => {
        throw new Error("mock snapshot failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "get_orchestrator_state",
        arguments: { format: "snapshot" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("state_read_failed");
  });

  it("returns state_read_failed when getGraph throws", async () => {
    const mockOrch = createMockOrch({
      getGraph: async () => {
        throw new Error("mock getGraph failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "get_orchestrator_state",
        arguments: { format: "graph" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("state_read_failed");
  });

  it("defaults to snapshot format when format is unknown", async () => {
    const mockOrch = createMockOrch();
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "get_orchestrator_state",
        arguments: {}, // no format → defaults to snapshot
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toHaveProperty("snapshot");
  });
});

// =========================================================================
// update_plan — catch path
// =========================================================================

describe("OrchToolProvider unit — update_plan", () => {
  it("returns ok:true even when updatePlan throws (best effort)", async () => {
    const mockOrch = createMockOrch({
      updatePlan: () => {
        throw new Error("mock updatePlan failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "update_plan",
        arguments: { plan: [{ step: 1 }] },
      } as any,
      makeCtx(),
    );

    // updatePlan is best-effort — errors are silently caught
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [{ step: 1 }] });
  });
});

// =========================================================================
// Unknown tool name
// =========================================================================

describe("OrchToolProvider unit — unknown tool", () => {
  it("returns unknown_tool for unregistered tool name", async () => {
    const mockOrch = createMockOrch();
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "completely_unknown_tool",
        arguments: {},
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("unknown_tool");
  });
});
