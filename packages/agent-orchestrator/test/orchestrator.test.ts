import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-agent-orchestrator";
import type { AgentSpec, EngineToolSet } from "piko-engine-protocol";

// Test tool sets
const codingToolSet: EngineToolSet = {
  id: "builtin:core-coding",
  name: "Core Coding",
  description: "Default coding tools.",
  tools: [
    {
      name: "shell",
      description: "Execute a shell command.",
      inputSchema: { type: "object", properties: {} },
      executor: { kind: "native", target: "shell" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["execute_process", "read_workspace"],
      approval: "always",
    },
    {
      name: "apply_patch",
      description: "Apply a structured patch.",
      inputSchema: { type: "object", properties: {} },
      executor: { kind: "native", target: "apply_patch" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["write_workspace"],
      approval: "always",
    },
  ],
};

const readOnlyShellToolSet: EngineToolSet = {
  id: "builtin:read-only-shell",
  name: "Read-Only Shell",
  tools: [
    {
      name: "shell",
      description: "Execute a read-only shell command.",
      inputSchema: { type: "object", properties: {} },
      executor: { kind: "native", target: "shell" },
      executionMode: "parallel",
      exposure: "direct",
      capabilities: ["read_workspace", "execute_process"],
      approval: "never",
    },
  ],
};

const _planningToolSet: EngineToolSet = {
  id: "builtin:planning",
  name: "Planning",
  tools: [
    {
      name: "update_plan",
      description: "Update the task plan.",
      inputSchema: { type: "object", properties: {} },
      executor: { kind: "host", target: "update_plan" },
      exposure: "direct",
      capabilities: ["update_plan"],
      approval: "never",
    },
  ],
};

const implementerSpec: AgentSpec = {
  id: "implementer",
  name: "Implementer",
  role: "Makes code changes.",
  systemPrompt: "Implement scoped changes using shell and apply_patch.",
  toolSetIds: ["builtin:core-coding"],
  concurrency: { requiresWriteLock: true, maxConcurrentTasks: 1 },
};

const reviewerSpec: AgentSpec = {
  id: "reviewer",
  name: "Reviewer",
  role: "Reviews code and reports issues.",
  systemPrompt: "Review code. Do not mutate files.",
  toolSetIds: ["builtin:read-only-shell"],
  concurrency: { canRunInParallel: true },
};

describe("AgentOrchestrator", () => {
  it("should create an orchestrator with initial state", () => {
    const orch = new AgentOrchestrator();
    const snap = orch.snapshot();
    expect(snap.status).toBe("idle");
    expect(Object.keys(snap.agents).length).toBe(0);
  });

  it("should register tool sets and agents", () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerToolSet(readOnlyShellToolSet);
    orch.registerAgent(implementerSpec);
    orch.registerAgent(reviewerSpec);

    const snap = orch.snapshot();
    expect(Object.keys(snap.toolSets).length).toBe(2);
    expect(Object.keys(snap.agents).length).toBe(2);
    expect(snap.agents.implementer.status).toBe("idle");
    expect(snap.agents.reviewer.status).toBe("idle");
  });

  it("should reject agents referencing unknown tool sets", () => {
    const orch = new AgentOrchestrator();
    expect(() =>
      orch.registerAgent({
        id: "bad",
        name: "Bad",
        role: "test",
        systemPrompt: "test",
        toolSetIds: ["nonexistent"],
      }),
    ).toThrow("unknown ToolSet");
  });

  it("should dispatch a task and emit events", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);

    const events: string[] = [];
    orch.subscribe((env) => {
      events.push(env.event.type);
    });
    orch.start();

    await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    expect(events).toContain("task_enqueued");
    expect(events).toContain("task_started");
    expect(events).toContain("scheduler_decision");

    const snap = orch.snapshot();
    const tasks = Object.values(snap.tasks);
    expect(tasks.length).toBe(1);
    expect(tasks[0].status).toBe("running");
  });

  it("should complete a task and emit events", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);
    orch.start();

    const events: string[] = [];
    const unsub = orch.subscribe((env) => {
      events.push(env.event.type);
    });

    const taskId = await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    orch.completeTask(taskId, { summary: "Done" });

    expect(events).toContain("task_completed");
    expect(events).toContain("agent_status_changed");

    const snap = orch.snapshot();
    expect(snap.tasks[taskId].status).toBe("completed");
    expect(snap.agents.implementer.status).toBe("idle");

    unsub();
  });

  it("should fail a task and emit events", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);
    orch.start();

    const taskId = await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    orch.failTask(taskId, "Something went wrong");

    const snap = orch.snapshot();
    expect(snap.tasks[taskId].status).toBe("failed");
    expect(snap.agents.implementer.status).toBe("failed");
  });

  it("should manage locks", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);
    orch.start();

    const taskId = await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    const acquired = orch.requestLock("implementer", taskId, "workspace", "write");
    expect(acquired).toBe(true);

    const snap = orch.snapshot();
    const lock = snap.locks["workspace-lock"];
    expect(lock).toBeDefined();
    expect(lock.holderAgentId).toBe("implementer");

    orch.releaseLock("implementer", taskId, "workspace");
    const snap2 = orch.snapshot();
    const lock2 = snap2.locks["workspace-lock"];
    expect(lock2.holderAgentId).toBeUndefined();
  });

  it("should block concurrent write-locked agents", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent({
      ...implementerSpec,
      id: "implementer-1",
    });
    orch.registerAgent({
      ...implementerSpec,
      id: "implementer-2",
    });
    orch.start();

    const task1Id = await orch.dispatch({
      targetAgentId: "implementer-1",
      prompt: "Task 1",
      source: { kind: "user" },
    });

    orch.requestLock("implementer-1", task1Id, "workspace", "write");

    // Dispatch task 2 (should be deferred because lock is held)
    const task2Id = await orch.dispatch({
      targetAgentId: "implementer-2",
      prompt: "Task 2",
      source: { kind: "user" },
    });

    // Task 2 should remain queued because impl-2 requires write lock and impl-1 holds it
    const snap = orch.snapshot();
    expect(snap.tasks[task2Id].status).toBe("queued");
  });

  it("should render graph projection", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);
    orch.start();

    const taskId = await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    orch.requestLock("implementer", taskId, "workspace", "write");

    const graph = orch.renderGraph();
    expect(graph.nodes.length).toBeGreaterThan(0);
    expect(graph.nodes.some((n) => n.kind === "agent")).toBe(true);
    expect(graph.nodes.some((n) => n.kind === "task")).toBe(true);
    expect(graph.nodes.some((n) => n.kind === "lock")).toBe(true);
    expect(graph.edges.length).toBeGreaterThan(0);
  });

  it("should dump events as append-only log", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementerSpec);
    orch.start();

    await orch.dispatch({
      targetAgentId: "implementer",
      prompt: "Write a hello world",
      source: { kind: "user" },
    });

    const events = orch.dumpEvents();
    expect(events.length).toBeGreaterThanOrEqual(4); // toolset_registered, agent_registered, started, task_enqueued, task_started, scheduler_decision
    const eventTypes = events.map((e) => e.event.type);
    expect(eventTypes).toContain("orchestrator_started");
  });
});
