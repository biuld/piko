// ---- State tests ----

import { describe, expect, it } from "bun:test";
import type { AgentSpec, AgentTask, HostEventListener, ToolSet } from "piko-orchestrator-protocol";
import { InMemoryEventStore } from "../../src/actors/state/event-store.js";
import type { OrchestratorEvent, OrchestratorEventEnvelope } from "../../src/actors/state/index.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";

// Import the internal state type for testing
type InternalState = {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  eventLog: OrchestratorEventEnvelope[];
  seq: number;
  agents: Record<string, import("piko-orchestrator-protocol").AgentRuntimeState>;
  tasks: Record<string, import("piko-orchestrator-protocol").AgentTaskState>;
  locks: Record<string, unknown>;
  listeners: Map<string, HostEventListener>;
  nextSubId: number;
  callMetas: Map<string, { name: string; args: Record<string, unknown> }>;
  toolSets: Record<string, ToolSet>;
};

function createTestStateActor(): {
  system: ActorSystem;
  stateId: string;
  state: InternalState;
  ingest: (event: OrchestratorEvent) => Promise<OrchestratorEventEnvelope>;
  snapshot: () => Promise<import("piko-orchestrator-protocol").OrchState>;
  dumpEvents: () => Promise<OrchestratorEventEnvelope[]>;
  getGraph: () => Promise<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }>;
  subscribe: (listener: HostEventListener) => Promise<{ id: string; unsubscribe: () => void }>;
  unsubscribe: (subscriptionId: string) => Promise<void>;
} {
  const system = new ActorSystem();
  const stateId = "event-store";
  const store = new InMemoryEventStore("test-run");
  const unsubscribers = new Map<string, () => void>();
  let nextUnsubId = 1;

  return {
    system,
    stateId,
    state: (store as any).state as any,
    ingest: async (event) => store.append(event),
    snapshot: async () => store.snapshot(),
    dumpEvents: async () => store.dumpEvents(),
    getGraph: async () => store.graph(),
    subscribe: async (listener) => {
      const unsub = store.subscribe(listener);
      const id = `sub_${nextUnsubId++}`;
      unsubscribers.set(id, unsub);
      return {
        id,
        unsubscribe: () => {
          unsub();
          unsubscribers.delete(id);
        },
      };
    },
    unsubscribe: async (subscriptionId) => {
      const unsub = unsubscribers.get(subscriptionId);
      if (unsub) {
        unsub();
        unsubscribers.delete(subscriptionId);
      }
    },
  };
}

// ---- Test helpers ----

function makeAgentSpec(id: string): AgentSpec {
  return {
    id,
    name: `Agent ${id}`,
    role: "test",
    systemPrompt: "You are a test agent.",
    toolSetIds: [],
  };
}

function makeTask(agentId: string, prompt: string): AgentTask {
  return {
    id: `task-${agentId}`,
    targetAgentId: agentId,
    prompt,
    source: { type: "user" },
  };
}

describe("StateActor", () => {
  // ---- Basic ingestion ----

  it("ingests an event and increments seq", async () => {
    const { ingest, state } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    expect(state.seq).toBe(1);
    expect(state.eventLog.length).toBe(1);
    expect(state.eventLog[0].seq).toBe(1);
    expect(state.eventLog[0].runId).toBe("test-run");
  });

  it("snapshot returns current state projection", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    const snap = await snapshot();

    expect(snap.status).toBe("running");
    expect(snap.runId).toBe("test-run");
  });

  it("snapshot returns a deep clone (immutable)", async () => {
    const { ingest, snapshot } = createTestStateActor();

    const spec = makeAgentSpec("agent-1");
    await ingest({ type: "agent_registered", agent: spec });

    const snap1 = await snapshot();
    // Mutating the snapshot should not affect internal state
    snap1.agents["agent-1"] = { id: "hacked", spec, status: "stopped", transcript: [] };

    const snap2 = await snapshot();
    expect(snap2.agents["agent-1"].id).toBe("agent-1");
    expect(snap2.agents["agent-1"].status).toBe("idle");
  });

  it("projects approval events with complete request context", async () => {
    const { ingest, subscribe } = createTestStateActor();
    const events: import("piko-orchestrator-protocol").HostEvent[] = [];
    await subscribe((event) => events.push(event));

    await ingest({
      type: "approval_requested",
      approvalId: "call-1",
      agentId: "agent-1",
      taskId: "task-1",
      toolName: "bash",
      toolArgs: { command: "ls" },
      eventSeq: 4,
      turnIndex: 2,
    });
    await ingest({
      type: "approval_resolved",
      approvalId: "call-1",
      agentId: "agent-1",
      taskId: "task-1",
      decision: "accept",
      eventSeq: 5,
      turnIndex: 2,
    });

    expect(events).toContainEqual(
      expect.objectContaining({
        type: "approval_needed",
        approvalId: "call-1",
        agentId: "agent-1",
        taskId: "task-1",
        toolName: "bash",
        toolArgs: { command: "ls" },
        eventSeq: 4,
        turnIndex: 2,
      }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "approval_resolved",
        approvalId: "call-1",
        agentId: "agent-1",
        taskId: "task-1",
        decision: "accept",
        eventSeq: 5,
        turnIndex: 2,
      }),
    );
  });

  // ---- Orchestrator lifecycle ----

  it("tracks orchestrator started → running status", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    expect((await snapshot()).status).toBe("running");
  });

  it("tracks orchestrator stopped → stopped status", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    await ingest({ type: "orchestrator_stopped", reason: "test" });
    expect((await snapshot()).status).toBe("stopped");
  });

  // ---- Agent registration ----

  it("agent_registered adds agent to state", async () => {
    const { ingest, snapshot } = createTestStateActor();

    const spec = makeAgentSpec("coordinator");
    await ingest({ type: "agent_registered", agent: spec });

    const snap = await snapshot();
    expect(snap.agents.coordinator).toBeDefined();
    expect(snap.agents.coordinator.spec.name).toBe("Agent coordinator");
    expect(snap.agents.coordinator.status).toBe("idle");
  });

  it("agent_unregistered removes agent", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    await ingest({ type: "agent_unregistered", agentId: "agent-1" });

    const snap = await snapshot();
    expect(snap.agents["agent-1"]).toBeUndefined();
  });

  // ---- Task lifecycle — full chain ----

  it("task_created adds task with queued status", async () => {
    const { ingest, snapshot } = createTestStateActor();

    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"]).toBeDefined();
    expect(snap.tasks["task-agent-1"].status).toBe("queued");
    expect(snap.tasks["task-agent-1"].prompt).toBe("Do something");
  });

  it("task_started sets task to running and agent to running", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "agent-1", taskId: "task-agent-1" });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"].status).toBe("running");
    expect(snap.agents["agent-1"].status).toBe("running");
    expect(snap.agents["agent-1"].activeTaskId).toBe("task-agent-1");
  });

  it("task_completed sets status and clears agent", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "agent-1", taskId: "task-agent-1" });
    await ingest({
      type: "task_completed",
      agentId: "agent-1",
      taskId: "task-agent-1",
      result: { summary: "Done!" },
    });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"].status).toBe("completed");
    expect(snap.tasks["task-agent-1"].result?.summary).toBe("Done!");
    expect(snap.agents["agent-1"].status).toBe("idle");
  });

  it("task_failed sets status and clears agent", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "agent-1", taskId: "task-agent-1" });
    await ingest({
      type: "task_failed",
      agentId: "agent-1",
      taskId: "task-agent-1",
      error: "boom",
    });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"].status).toBe("failed");
    expect(snap.tasks["task-agent-1"].error).toBe("boom");
    expect(snap.agents["agent-1"].status).toBe("idle");
  });

  it("task_cancelled sets status with reason", async () => {
    const { ingest, snapshot } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "agent-1", taskId: "task-agent-1" });
    await ingest({
      type: "task_cancelled",
      agentId: "agent-1",
      taskId: "task-agent-1",
      reason: "User requested",
    });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"].status).toBe("cancelled");
    expect(snap.tasks["task-agent-1"].error).toBe("User requested");
  });

  // ---- Plan updates ----

  it("plan_updated stores the task plan and emits it to host subscribers", async () => {
    const { ingest, snapshot, subscribe } = createTestStateActor();
    const received: Parameters<HostEventListener>[0][] = [];
    await subscribe((event) => received.push(event));

    const task = makeTask("agent-1", "Do something");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "agent-1", taskId: "task-agent-1" });
    await ingest({
      type: "plan_updated",
      agentId: "agent-1",
      taskId: "task-agent-1",
      plan: [{ step: 1 }, { step: 2 }],
    });

    const snap = await snapshot();
    expect(snap.tasks["task-agent-1"].plan).toEqual([{ step: 1 }, { step: 2 }]);
    expect(received.find((event) => event.type === "plan_updated")).toMatchObject({
      type: "plan_updated",
      agentId: "agent-1",
      taskId: "task-agent-1",
      plan: [{ step: 1 }, { step: 2 }],
    });
  });

  // ---- Tool lifecycle ----

  it("tool_started and tool_finished update callMetas", async () => {
    const { ingest, state } = createTestStateActor();

    await ingest({
      type: "tool_started",
      agentId: "agent-1",
      taskId: "task-1",
      callId: "call-1",
      name: "bash",
      args: { command: "ls" },
    });

    expect(state.callMetas.get("call-1")).toEqual({ name: "bash", args: { command: "ls" } });

    // tool_finished doesn't clear the meta (HostEvent mapping needs it)
    await ingest({
      type: "tool_finished",
      agentId: "agent-1",
      taskId: "task-1",
      callId: "call-1",
      result: { ok: true, value: "file.txt" },
    });

    expect(state.callMetas.has("call-1")).toBe(true);
  });

  // ---- Event log ----

  it("dump_events returns full event log", async () => {
    const { ingest, dumpEvents } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    await ingest({ type: "agent_registered", agent: makeAgentSpec("a") });
    await ingest({ type: "agent_registered", agent: makeAgentSpec("b") });

    const events = await dumpEvents();
    expect(events.length).toBe(3);
    expect(events[0].seq).toBe(1);
    expect(events[1].seq).toBe(2);
    expect(events[2].seq).toBe(3);
  });

  // ---- Subscriptions ----

  it("subscribe notifies listener of HostEvents", async () => {
    const { ingest, subscribe } = createTestStateActor();
    const received: Array<{ type: string }> = [];

    const listener: HostEventListener = (event) => {
      received.push(event as { type: string });
    };

    await subscribe(listener);

    // Events that produce HostEvents
    await ingest({ type: "agent_registered", agent: makeAgentSpec("a") });
    await ingest({
      type: "task_started",
      agentId: "agent-a",
      taskId: "task-1",
    });

    // task_started should produce a HostEvent
    expect(received.some((e) => e.type === "task_started")).toBe(true);
  });

  it("unsubscribe removes listener", async () => {
    const { ingest, subscribe, unsubscribe } = createTestStateActor();
    const received: Array<{ type: string }> = [];

    const listener: HostEventListener = (event) => {
      received.push(event as { type: string });
    };

    const sub = await subscribe(listener);
    await unsubscribe(sub.id);

    // After unsubscribing, no more events should be received
    await ingest({ type: "task_started", agentId: "a", taskId: "t1" });
    expect(received.length).toBe(0);
  });

  // ---- Graph projection ----

  it("render_graph returns nodes for agents and tasks", async () => {
    const { ingest, getGraph } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("coordinator") });
    const task = makeTask("coordinator", "Build feature X");
    await ingest({ type: "task_created", task });
    await ingest({ type: "task_started", agentId: "coordinator", taskId: "task-coordinator" });

    const graph = await getGraph();
    expect(graph.nodes.length).toBeGreaterThanOrEqual(2);
    expect(graph.nodes.find((n) => n.id === "agent:coordinator")).toBeDefined();
    expect(graph.nodes.find((n) => n.id === "task:task-coordinator")).toBeDefined();

    // Edge from agent to active task
    expect(
      graph.edges.some((e) => e.from === "agent:coordinator" && e.to === "task:task-coordinator"),
    ).toBe(true);
  });

  it("render_graph includes parent-child task edges", async () => {
    const { ingest, getGraph } = createTestStateActor();

    await ingest({ type: "agent_registered", agent: makeAgentSpec("agent-1") });
    const parent: AgentTask = {
      id: "task-parent",
      targetAgentId: "agent-1",
      prompt: "Parent",
      source: { type: "user" },
    };
    const child: AgentTask = {
      id: "task-child",
      targetAgentId: "agent-1",
      prompt: "Child",
      source: { type: "user" },
      parentTaskId: "task-parent",
    };

    await ingest({ type: "task_created", task: parent });
    await ingest({ type: "task_created", task: child });

    const graph = await getGraph();
    expect(
      graph.edges.some((e) => e.from === "task:task-parent" && e.to === "task:task-child"),
    ).toBe(true);
  });

  // ---- Deterministic reducer ----

  it("seq is monotonically increasing across multiple events", async () => {
    const { ingest, state } = createTestStateActor();

    await ingest({ type: "orchestrator_started" });
    await ingest({ type: "orchestrator_started" });
    await ingest({ type: "orchestrator_started" });

    expect(state.eventLog[0].seq).toBe(1);
    expect(state.eventLog[1].seq).toBe(2);
    expect(state.eventLog[2].seq).toBe(3);
  });

  // ---- Concurrency (actor FIFO) ----

  it("handles multiple concurrent ingest calls without race conditions", async () => {
    const { ingest, state } = createTestStateActor();

    // Fire multiple ingests concurrently
    await Promise.all([
      ingest({ type: "orchestrator_started" }),
      ingest({ type: "orchestrator_started" }),
      ingest({ type: "orchestrator_started" }),
    ]);

    // All 3 events should be logged in order
    expect(state.eventLog.length).toBe(3);
    expect(state.seq).toBe(3);
  });

  // ---- task_delta events ----

  it("task_delta events are stored in event log", async () => {
    const { ingest, dumpEvents } = createTestStateActor();

    await ingest({
      type: "task_delta",
      agentId: "agent-1",
      taskId: "task-1",
      delta: { kind: "text", text: "Hello" },
    });

    await ingest({
      type: "task_delta",
      agentId: "agent-1",
      taskId: "task-1",
      delta: { kind: "thinking", text: "Hmm..." },
    });

    const events = await dumpEvents();
    expect(events.length).toBe(2);
  });
});
