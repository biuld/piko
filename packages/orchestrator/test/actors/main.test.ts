// ---- MainActor tests ----

import { describe, expect, it } from "bun:test";
import type { AgentSpec, AgentTask } from "piko-orchestrator-protocol";
import type { AgentActorDeps } from "../../src/actors/agent/index.js";
import { createMainActor } from "../../src/actors/main.js";
import type { OrchestratorEvent } from "../../src/actors/state.js";
import type { ActorHandler } from "../../src/kernel/actor-system.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";
import { ToolRegistryImpl } from "../../src/tools/index.js";
import { createFauxModelExecutor } from "../helpers/index.js";

// ---- Helpers ----

function makeAgentSpec(id: string, overrides?: Partial<AgentSpec>): AgentSpec {
  return {
    id,
    name: `Agent ${id}`,
    role: "test",
    systemPrompt: "You are a test agent.",
    toolSetIds: [],
    ...overrides,
  };
}

function makeTask(prompt: string, targetAgentId?: string): AgentTask {
  return {
    id: `task-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    targetAgentId: targetAgentId ?? "main",
    prompt,
    source: { type: "user" },
  };
}

async function createTestMainActor() {
  const system = new ActorSystem();
  const emitted: OrchestratorEvent[] = [];
  const emit = async (event: OrchestratorEvent) => {
    emitted.push(event);
  };

  // ---- StateActor ----
  system.spawn({
    id: "orchestrator:state",
    kind: "state",
    handler: (async (
      msg: unknown,
      ctx: import("../../src/kernel/actor-system.js").ActorContext,
      meta: import("../../src/kernel/envelope.js").Envelope,
    ) => {
      const m = msg as { type: string; event?: OrchestratorEvent; listener?: unknown };
      if (m.type === "ingest_event" && m.event) {
        emitted.push(m.event);
        ctx.reply(meta, { id: "evt", runId: "test", seq: 1, time: Date.now(), event: m.event });
      } else if (m.type === "snapshot") {
        ctx.reply(meta, { runId: "test", status: "idle", agents: {}, tasks: {} });
      } else if (m.type === "render_graph") {
        ctx.reply(meta, { nodes: [], edges: [] });
      } else {
        ctx.reply(meta, undefined);
      }
    }) as ActorHandler,
  });

  // ---- Mock Faux agent actor response ----
  // We spawn a handler that replies to dispatch with a result
  // This simulates what a real AgentActor would do

  // ---- MainActor ----
  const modelExecutor = createFauxModelExecutor({
    steps: [{ content: "Task completed.", status: "completed" }],
  });

  const toolRegistry = new ToolRegistryImpl(system, emit);

  const mainActorState = createMainActor({
    actorSystem: system,
    stateActorId: "orchestrator:state",
    emit,
    createAgentDeps: (): AgentActorDeps => ({
      modelExecutor,
      emit,
      maxSteps: 50,
      actorSystem: system,
      modelConfig: undefined,
      toolRegistry,
    }),
  });

  system.spawn({
    id: "orchestrator:main",
    kind: "main",
    handler: mainActorState.handler as ActorHandler,
  });

  return {
    system,
    mainActorState,
    emitted,
    registerAgent: (spec: AgentSpec) =>
      system.ask("orchestrator:main", { type: "register_agent", spec }),
    unregisterAgent: (agentId: string) =>
      system.ask("orchestrator:main", { type: "unregister_agent", agentId }),
    dispatch: (task: AgentTask) =>
      system.ask<{ taskId: string }>("orchestrator:main", { type: "dispatch", task }),
    run: (prompt: string, options?: { targetAgentId?: string; signal?: AbortSignal }) =>
      system.ask<import("piko-orchestrator-protocol").OrchRunResult>("orchestrator:main", {
        type: "run",
        prompt,
        options,
      }),
    cancelTask: (taskId: string, reason?: string) =>
      system.ask("orchestrator:main", { type: "cancel_task", taskId, reason }),
    setModelConfig: (config: unknown) =>
      system.ask("orchestrator:main", { type: "set_model_config", config }),
  };
}

describe("MainActor", () => {
  // ---- Agent registration ----

  it("register_agent emits agent_registered event", async () => {
    const { registerAgent, emitted } = await createTestMainActor();

    await registerAgent(makeAgentSpec("coordinator"));

    const event = emitted.find((e) => e.type === "agent_registered");
    expect(event).toBeDefined();
  });

  it("register_agent spawns the agent actor", async () => {
    const { registerAgent, system } = await createTestMainActor();

    await registerAgent(makeAgentSpec("implementer"));

    expect(system.hasActor("agent:implementer")).toBe(true);
  });

  it("unregister_agent emits agent_unregistered and stops agent", async () => {
    const { registerAgent, unregisterAgent, system, emitted } = await createTestMainActor();

    await registerAgent(makeAgentSpec("temp-agent"));
    expect(system.hasActor("agent:temp-agent")).toBe(true);

    await unregisterAgent("temp-agent");

    const event = emitted.find((e) => e.type === "agent_unregistered");
    expect(event).toBeDefined();
    expect(system.hasActor("agent:temp-agent")).toBe(false);
  });

  it("unregister_agent cleans up task owners", async () => {
    const { registerAgent, dispatch, unregisterAgent, mainActorState } =
      await createTestMainActor();

    await registerAgent(makeAgentSpec("worker"));
    const task = makeTask("Do work", "worker");
    await dispatch(task);

    // task should be in taskOwners
    expect(mainActorState.state.taskOwners.has(task.id!)).toBe(true);

    await unregisterAgent("worker");
    expect(mainActorState.state.taskOwners.has(task.id!)).toBe(false);
  });

  // ---- Dispatch ----

  it("dispatch routes task to agent and emits task_created", async () => {
    const { registerAgent, dispatch, emitted } = await createTestMainActor();

    await registerAgent(makeAgentSpec("worker"));
    const task = makeTask("Do something", "worker");

    const result = await dispatch(task);

    expect(result).toBeDefined();
    const createdEvent = emitted.find((e) => e.type === "task_created");
    expect(createdEvent).toBeDefined();
  });

  it("dispatch records task ownership", async () => {
    const { registerAgent, dispatch, mainActorState } = await createTestMainActor();

    await registerAgent(makeAgentSpec("worker"));
    const task = makeTask("Do work", "worker");

    await dispatch(task);

    // taskOwners should map taskId -> agentId
    expect(mainActorState.state.taskOwners.get(task.id!)).toBe("worker");
  });

  it("dispatch uses defaultAgentId when target not specified", async () => {
    const { registerAgent, dispatch, mainActorState } = await createTestMainActor();

    const spec = makeAgentSpec("main");
    await registerAgent(spec);

    // Set main as default
    mainActorState.state.defaultAgentId = "main";

    const task: AgentTask = {
      prompt: "Hello",
      source: { type: "user" },
      targetAgentId: "", // Will be overridden
    };

    const result = await dispatch(task);
    expect(result).toBeDefined();
  });

  // ---- Run ----

  it("run creates task and returns result", async () => {
    const { registerAgent, run } = await createTestMainActor();

    await registerAgent(makeAgentSpec("assistant"));

    const result = await run("Say hello", { targetAgentId: "assistant" });

    expect(result.status).toBeDefined();
    expect(result.messages).toBeDefined();
  });

  it("run throws when agent is not registered", async () => {
    const { run } = await createTestMainActor();

    await expect(run("Do something", { targetAgentId: "ghost" })).rejects.toThrow(
      'Agent "ghost" not registered',
    );
  });

  it("run emits orchestrator_started and task_created events", async () => {
    const { registerAgent, run, emitted } = await createTestMainActor();

    await registerAgent(makeAgentSpec("assistant"));

    await run("Hello", { targetAgentId: "assistant" });

    expect(emitted.some((e) => e.type === "orchestrator_started")).toBe(true);
    expect(emitted.some((e) => e.type === "task_created")).toBe(true);
  });

  // ---- Cancel ----

  it("cancel_task routes to the owning agent", async () => {
    const { registerAgent, dispatch, cancelTask } = await createTestMainActor();

    await registerAgent(makeAgentSpec("worker"));
    const task = makeTask("Long work", "worker");
    await dispatch(task);

    // Cancel should route to the agent actor
    const result = await cancelTask(task.id!);
    expect(result).toBeUndefined();
  });

  it("cancel_task throws for unknown task", async () => {
    const { cancelTask } = await createTestMainActor();

    await expect(cancelTask("nonexistent-task")).rejects.toThrow(
      'Task "nonexistent-task" not found',
    );
  });

  // ---- set_model_config ----

  it("set_model_config stores and forwards config to agents", async () => {
    const { registerAgent, setModelConfig, mainActorState } = await createTestMainActor();

    await registerAgent(makeAgentSpec("agent-1"));

    const config = { model: { id: "gpt-4", name: "GPT-4" } };
    await setModelConfig(config);

    expect(mainActorState.state.latestModelConfig).toEqual(config);
  });
});
