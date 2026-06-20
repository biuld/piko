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
// delegate_to_agent — all edge cases
// =========================================================================

describe("OrchToolProvider unit — delegate_to_agent", () => {
  // ---- missing args ----

  it("returns invalid_args when agentId is missing", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { prompt: "do work" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when prompt is missing", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: "implementer" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  // ---- type errors ----

  it("returns invalid_args when agentId is null", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: null, prompt: "ok" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when agentId is a number", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: 42, prompt: "ok" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when agentId is an object", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: {}, prompt: "ok" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when prompt is a number", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: "x", prompt: 123 } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  // ---- empty / blank strings ----

  it("returns invalid_args when agentId is empty string", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: "", prompt: "ok" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when prompt is empty string", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: "x", prompt: "" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when agentId is only whitespace", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "delegate_to_agent", arguments: { agentId: "   ", prompt: "ok" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when prompt is only whitespace", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "x", prompt: "  \t\n  " },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  // ---- mode validation ----

  it("defaults to call mode when mode is not provided", async () => {
    const mockOrch = createMockOrch({
      delegateToAgent: async () => {
        return { taskId: "task-1", result: { summary: "done" } };
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "do work" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect((result.value as { mode?: string })?.mode).toBe("call");
  });

  it("returns invalid_args for illegal mode 'async'", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "ok", mode: "async" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args for illegal mode '' (empty string)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "ok", mode: "" },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args for illegal mode (number)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      {
        id: "c1",
        name: "delegate_to_agent",
        arguments: { agentId: "implementer", prompt: "ok", mode: 123 },
      } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  // ---- detach throws (agent_busy code) ----

  it("returns agent_busy when delegateDetached throws with code agent_busy", async () => {
    const busyErr = Object.assign(new Error("Agent is busy"), { code: "agent_busy" });
    const mockOrch = createMockOrch({
      delegateDetached: async () => {
        throw busyErr;
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
    expect(result.error?.code).toBe("agent_busy");
  });

  it("returns concurrency_limit when delegateDetached throws with code concurrency_limit", async () => {
    const limitErr = Object.assign(new Error("Concurrency limit"), { code: "concurrency_limit" });
    const mockOrch = createMockOrch({
      delegateDetached: async () => {
        throw limitErr;
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
    expect(result.error?.code).toBe("concurrency_limit");
  });

  // ---- call throws (agent_busy code) ----

  it("returns agent_busy when delegateToAgent throws with code agent_busy", async () => {
    const busyErr = Object.assign(new Error("Agent busy"), { code: "agent_busy" });
    const mockOrch = createMockOrch({
      delegateToAgent: async () => {
        throw busyErr;
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
    expect(result.error?.code).toBe("agent_busy");
  });

  it("returns concurrency_limit when delegateToAgent throws with code concurrency_limit", async () => {
    const limitErr = Object.assign(new Error("Limit"), { code: "concurrency_limit" });
    const mockOrch = createMockOrch({
      delegateToAgent: async () => {
        throw limitErr;
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
    expect(result.error?.code).toBe("concurrency_limit");
  });

  // ---- non-Error thrown ----

  it("uses fallback message when delegateDetached throws a non-Error value", async () => {
    const mockOrch = createMockOrch({
      delegateDetached: async () => {
        throw "raw string";
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
    expect(result.error?.message).toBe("Delegation failed");
  });

  it("uses fallback message when delegateToAgent throws a non-Error value", async () => {
    const mockOrch = createMockOrch({
      delegateToAgent: async () => {
        throw 42;
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
    expect(result.error?.message).toBe("Delegation failed");
  });

  // ---- success call mode ----

  it("returns full result on successful call delegation", async () => {
    const mockOrch = createMockOrch({
      delegateToAgent: async () => ({
        taskId: "task-call-1",
        result: { summary: "All done!", artifacts: [{ id: "a1", type: "file", data: {} }] },
      }),
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

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({
      delegated: true,
      taskId: "task-call-1",
      targetAgentId: "implementer",
      mode: "call",
      result: { summary: "All done!", artifacts: [{ id: "a1", type: "file", data: {} }] },
    });
  });

  // ---- success detach mode ----

  it("returns handle on successful detach delegation", async () => {
    const mockOrch = createMockOrch({
      delegateDetached: async () => "task-detach-1",
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

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({
      delegated: true,
      taskId: "task-detach-1",
      targetAgentId: "implementer",
      mode: "detach",
    });
  });

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

  // ---- busy agent via snapshot ----

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
// join_subtask — all edge cases
// =========================================================================

describe("OrchToolProvider unit — join_subtask", () => {
  // ---- missing / invalid args ----

  it("returns invalid_args when taskId is missing", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: {} } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when taskId is null", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: null } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when taskId is a number", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: 42 } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when taskId is an object", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: {} } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when taskId is empty string", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: "" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  it("returns invalid_args when taskId is only whitespace", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: "   " } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("invalid_args");
  });

  // ---- success ----

  it("returns full result on successful join", async () => {
    const mockOrch = createMockOrch({
      joinTask: async () => ({ summary: "Work complete", artifacts: [] }),
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: "task-1" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({
      joined: true,
      taskId: "task-1",
      result: { summary: "Work complete", artifacts: [] },
    });
  });

  // ---- throw Error ----

  it("returns join_failed when joinTask throws Error", async () => {
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

  // ---- throw non-Error ----

  it("returns join_failed with fallback message when joinTask throws non-Error", async () => {
    const mockOrch = createMockOrch({
      joinTask: async () => {
        throw "raw error string";
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      { id: "c1", name: "join_subtask", arguments: { taskId: "task-1" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(false);
    expect(result.error?.code).toBe("join_failed");
    expect(result.error?.message).toBe("Join failed");
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
// update_plan — all edge cases
// =========================================================================

describe("OrchToolProvider unit — update_plan", () => {
  // ---- normal plan ----

  it("uses context agentId and taskId for normal plan", async () => {
    let capturedAgentId: string | undefined;
    let capturedTaskId: string | undefined;
    let capturedPlan: unknown[] | undefined;
    const mockOrch = createMockOrch({
      updatePlan: (agentId, taskId, plan) => {
        capturedAgentId = agentId;
        capturedTaskId = taskId;
        capturedPlan = plan;
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const plan = [{ step: "Analyze" }, { step: "Build" }];
    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan } } as any,
      makeCtx({ agentId: "caller-1", taskId: "task-caller-1" }),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan });
    expect(capturedAgentId).toBe("caller-1");
    expect(capturedTaskId).toBe("task-caller-1");
    expect(capturedPlan).toEqual(plan);
  });

  // ---- empty array ----

  it("handles empty array", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: [] } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [] });
  });

  // ---- missing plan ----

  it("handles missing plan as empty array (best-effort compat)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: {} } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [] });
  });

  // ---- null ----

  it("converts null to empty array (best-effort compat)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: null } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [] });
  });

  // ---- string ----

  it("converts string to empty array (best-effort compat)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: "not-an-array" } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [] });
  });

  // ---- object ----

  it("converts object to empty array (best-effort compat)", async () => {
    const provider = new OrchToolProvider(createMockOrch());

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: { step: 1 } } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [] });
  });

  // ---- array containing primitive/null ----

  it("passes through array containing primitives (no validation)", async () => {
    let capturedPlan: unknown[] | undefined;
    const mockOrch = createMockOrch({
      updatePlan: (_a, _t, plan) => {
        capturedPlan = plan;
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const plan = [1, "string", null, true];
    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan });
    expect(capturedPlan).toEqual(plan);
  });

  // ---- updatePlan throws Error ----

  it("returns ok:true even when updatePlan throws Error (best effort)", async () => {
    const mockOrch = createMockOrch({
      updatePlan: () => {
        throw new Error("mock updatePlan failure");
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: [{ step: 1 }] } } as any,
      makeCtx(),
    );

    // updatePlan is best-effort — errors are silently caught
    expect(result.ok).toBe(true);
    expect(result.value).toEqual({ updated: true, plan: [{ step: 1 }] });
  });

  // ---- updatePlan throws non-Error ----

  it("returns ok:true even when updatePlan throws non-Error (best effort)", async () => {
    const mockOrch = createMockOrch({
      updatePlan: () => {
        throw 42;
      },
    });
    const provider = new OrchToolProvider(mockOrch);

    const result = await provider.execute(
      { id: "c1", name: "update_plan", arguments: { plan: [{ step: 1 }] } } as any,
      makeCtx(),
    );

    expect(result.ok).toBe(true);
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
