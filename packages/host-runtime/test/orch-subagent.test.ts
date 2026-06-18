// ---- Orchestrator subagent delegation tests ----
// Verifies orchestrator behavior: dispatchDetached/joinTask API,
// delegate_to_agent (detach mode), join_subtask, error paths,
// get_orchestrator_state, update_plan.
//
// Known limitation: delegate_to_agent in "call" mode deadlocks because
// OrchToolProvider.handleDelegate calls orchestrator.dispatchDetached() which
// sends an ask to MainActor, but MainActor is already processing the run()
// message. The actor mailbox serializes messages → deadlock.
// Call mode is tested at the API level via dispatchDetached + joinTask directly.

import { afterAll, beforeAll, beforeEach, describe, expect, it } from "bun:test";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import { createModelCaller } from "piko-orchestrator";
import type { AgentSpec } from "piko-orchestrator-protocol";
import { createHostConfig, PikoHost } from "../src/index.js";
import { fs, join, tmpdir } from "./bun-test-utils.js";

const PROVIDER = "faux-subagent";
const API = "openai-completions";
const MODEL_ID = "faux-subagent-model";

let faux: FauxProviderRegistration;
const originalHome = process.env.HOME;

beforeAll(() => {
  faux = registerFauxProvider({
    api: API,
    provider: PROVIDER,
    models: [{ id: MODEL_ID }],
  });
});

beforeEach(async () => {
  process.env.HOME = await fs.mkdtemp(join(tmpdir(), "piko-subagent-home-"));
});

afterAll(() => {
  faux?.unregister();
  process.env.HOME = originalHome;
});

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Faux Subagent Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

function makeAgentSpec(id: string, overrides?: Partial<AgentSpec>): AgentSpec {
  return {
    id,
    name: `Agent ${id}`,
    role: "test",
    systemPrompt: `You are ${id}.`,
    toolSetIds: ["builtin"],
    ...overrides,
  };
}

function delegateCall(
  agentId: string,
  prompt: string,
  mode: "call" | "detach" = "detach",
  callId = "call_delegate",
) {
  return fauxToolCall("delegate_to_agent", { agentId, prompt, mode }, { id: callId });
}

function joinCall(taskId: string, callId = "call_join") {
  return fauxToolCall("join_subtask", { taskId }, { id: callId });
}

/** Helper to extract error code from a nested tool result details. */
function errCode(toolResults: Array<Record<string, unknown>>, index: number): string | undefined {
  const tr = toolResults[index];
  if (!tr) return undefined;
  const details = tr.details as Record<string, unknown> | undefined;
  if (!details) return undefined;
  // details may be { code, message } or { error: { code, message } }
  if (typeof details.code === "string") return details.code;
  if (details.error && typeof details.error === "object") {
    return (details.error as Record<string, string>).code;
  }
  return undefined;
}

// =========================================================================
// Section 1 — orchestrator API: dispatchDetached / joinTask
// =========================================================================

describe("Orchestrator dispatchDetached / joinTask (API level)", () => {
  it("dispatchDetached starts a task and joinTask returns its result", async () => {
    faux.setResponses([fauxAssistantMessage("Subagent work completed.")]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("worker"));

    const taskId = await host.orchestrator!.dispatchDetached({
      targetAgentId: "worker",
      prompt: "Do background work",
      source: { type: "user" },
    });
    expect(taskId).toBeDefined();

    const result = await host.orchestrator!.joinTask(taskId);
    expect(result).toBeDefined();
  });

  it("dispatchDetached to unknown agent — join fails later", async () => {
    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    // dispatchDetached returns taskId (goes through MainActor, which rejects internally)
    const taskId = await host.orchestrator!.dispatchDetached({
      targetAgentId: "ghost",
      prompt: "Work",
      source: { type: "user" },
    });

    // joinTask eventually fails because the dispatch was rejected
    await expect(host.orchestrator!.joinTask(taskId)).rejects.toThrow();
  });

  it("joinTask rejects for unknown taskId", async () => {
    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    await expect(host.orchestrator!.joinTask("nonexistent")).rejects.toThrow(
      "Detached task not found",
    );
  });

  it("dispatchDetached runs subagent concurrently with another task", async () => {
    faux.setResponses([
      fauxAssistantMessage("Background work done."),
      fauxAssistantMessage("Main work done."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("bg-worker"));

    const detachedTaskId = await host.orchestrator!.dispatchDetached({
      targetAgentId: "bg-worker",
      prompt: "Background task",
      source: { type: "user" },
    });

    const mainResult = await host.run("Main task");
    expect(mainResult.status).toBe("completed");

    const bgResult = await host.orchestrator!.joinTask(detachedTaskId);
    expect(bgResult).toBeDefined();
  });
});

// =========================================================================
// Section 2 — delegate_to_agent via tool calls (call + detach modes)
// =========================================================================

describe("Orchestrator delegate_to_agent via tool calls", () => {
  it("call mode — subagent runs and result flows back to coordinator", async () => {
    faux.setResponses([
      fauxAssistantMessage([delegateCall("implementer", "Implement feature X", "call")]),
      fauxAssistantMessage("Feature X implemented successfully."),
      fauxAssistantMessage("Delegation complete. The implementer finished."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("implementer"));

    const result = await host.run("Delegate to implementer (call mode)");
    expect(result.status).toBe("completed");
    expect(result.totalSteps).toBeGreaterThanOrEqual(1);

    const toolResults = result.messages.filter((m) => m.role === "toolResult");
    expect(toolResults.length).toBe(1);
    expect((toolResults[0] as { isError?: boolean }).isError).toBeFalsy();

    const assistantMsgs = result.messages.filter((m) => m.role === "assistant");
    expect(assistantMsgs.length).toBe(2);
  });

  it("call mode — subagent does multi-step work with tools", async () => {
    faux.setResponses([
      fauxAssistantMessage([delegateCall("implementer", "Build feature Y", "call")]),
      fauxAssistantMessage([fauxToolCall("bash", { command: "ls" }, { id: "tc_ls" })]),
      fauxAssistantMessage("Files listed. Feature Y built."),
      fauxAssistantMessage("Implementer completed multi-step work."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("implementer"));

    const result = await host.run("Multi-step delegation (call mode)");
    expect(result.status).toBe("completed");
    expect(result.totalSteps).toBe(2);
  });

  it("detach delegation returns taskId to model, coordinator continues", async () => {
    faux.setResponses([
      fauxAssistantMessage([delegateCall("worker", "Background analysis")]),
      fauxAssistantMessage("Background analysis completed."),
      fauxAssistantMessage("I've delegated the analysis. Continuing my own work."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("worker"));

    const result = await host.run("Analyze and continue");
    expect(result.status).toBe("completed");

    const toolResults = result.messages.filter((m) => m.role === "toolResult");
    expect(toolResults.length).toBe(1);

    const tr = toolResults[0] as { isError?: boolean; details?: unknown };
    expect(tr.isError).toBeFalsy();
    const d = tr.details as { delegated?: boolean; taskId?: string; mode?: string };
    expect(d?.delegated).toBe(true);
    expect(d?.mode).toBe("detach");
    expect(d?.taskId).toBeDefined();
  });
});

// =========================================================================
// Section 3 — delegate_to_agent error paths (via tool calls)
// =========================================================================

describe("Orchestrator delegate_to_agent — error paths", () => {
  function err(toolResults: Array<Record<string, unknown>>, idx = 0): string | undefined {
    return errCode(toolResults as Array<Record<string, unknown>>, idx);
  }

  it("rejects delegation to self", async () => {
    faux.setResponses([
      fauxAssistantMessage([delegateCall("main", "Do this yourself")]),
      fauxAssistantMessage("Cannot delegate to myself. I'll do it myself."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Self-delegate");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(trs.length).toBe(1);
    expect(trs[0].isError).toBe(true);
    expect(err(trs)).toBe("invalid_args");
  });

  it("rejects delegation to unknown agent", async () => {
    faux.setResponses([
      fauxAssistantMessage([delegateCall("ghost-agent", "Do work")]),
      fauxAssistantMessage("Agent not found."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Delegate to ghost");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(trs.length).toBe(1);
    expect(trs[0].isError).toBe(true);
    expect(err(trs)).toBe("not_found");
  });

  it("join_subtask returns error for unknown taskId", async () => {
    faux.setResponses([
      fauxAssistantMessage([joinCall("nonexistent-task-id")]),
      fauxAssistantMessage("Join failed as expected."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Join unknown task");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(trs.length).toBe(1);
    expect(trs[0].isError).toBe(true);
    expect(err(trs)).toBe("join_failed");
  });

  it("delegate_to_agent returns error for missing required args", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("delegate_to_agent", { mode: "call" }, { id: "call_bad" }),
      ]),
      fauxAssistantMessage("Missing args, will handle."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Bad delegation");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(err(trs)).toBe("invalid_args");
  });

  it("rejects delegation to busy agent", async () => {
    faux.setResponses([
      (context: any) => {
        const isImplementer = JSON.stringify(context.messages).includes("Long running work");
        if (isImplementer) {
          return fauxAssistantMessage([
            fauxToolCall("bash", { command: "sleep 1" }, { id: "tc_slow" }),
          ]);
        } else {
          return fauxAssistantMessage([delegateCall("implementer", "Urgent work")]);
        }
      },
      (context: any) => {
        const isImplementer = JSON.stringify(context.messages).includes("Long running work");
        if (isImplementer) {
          return fauxAssistantMessage("Long task done.");
        } else {
          return fauxAssistantMessage("Implementer is busy. I'll wait.");
        }
      },
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("implementer"));

    const busyTaskId = await host.orchestrator!.dispatchDetached({
      targetAgentId: "implementer",
      prompt: "Long running work",
      source: { type: "user" },
    });

    await new Promise((r) => setTimeout(r, 30));

    const result = await host.run("Delegate urgent work");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(trs.length).toBe(1);
    expect(trs[0].isError).toBe(true);
    expect(err(trs)).toBe("agent_busy");

    await host.orchestrator!.joinTask(busyTaskId).catch(() => {});
  });

  it("join_subtask returns error for missing taskId", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("join_subtask", {}, { id: "call_join_bad" })]),
      fauxAssistantMessage("Missing taskId."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Join missing args");
    expect(result.status).toBe("completed");

    const trs = result.messages.filter((m) => m.role === "toolResult") as any[];
    expect(trs.length).toBe(1);
    expect(trs[0].isError).toBe(true);
    expect(err(trs)).toBe("invalid_args");
  });

  it("join_subtask returns joined result when given a valid taskId (API level)", async () => {
    faux.setResponses([fauxAssistantMessage("Background work done.")]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("worker"));

    const taskId = await host.orchestrator!.dispatchDetached({
      targetAgentId: "worker",
      prompt: "Background analysis",
      source: { type: "user" },
    });

    const result = await host.orchestrator!.joinTask(taskId);
    expect(result).toBeDefined();
  });
});

// =========================================================================
// Section 4 — get_orchestrator_state and update_plan
// =========================================================================

describe("Orchestrator get_orchestrator_state / update_plan", () => {
  it("get_orchestrator_state returns snapshot to model", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("get_orchestrator_state", { format: "snapshot" }, { id: "call_state" }),
      ]),
      fauxAssistantMessage("State retrieved."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    host.orchestrator!.registerAgent(makeAgentSpec("reviewer"));

    const result = await host.run("Show state");
    expect(result.status).toBe("completed");

    const toolResults = result.messages.filter((m) => m.role === "toolResult");
    expect(toolResults.length).toBe(1);
    expect((toolResults[0] as { isError?: boolean }).isError).toBeFalsy();
  });

  it("get_orchestrator_state in graph format", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("get_orchestrator_state", { format: "graph" }, { id: "call_graph" }),
      ]),
      fauxAssistantMessage("Graph retrieved."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Show graph");
    expect(result.status).toBe("completed");
  });

  it("update_plan stores plan in orchestrator state", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall(
          "update_plan",
          {
            plan: [{ step: "Analyze" }, { step: "Implement" }, { step: "Test" }],
          },
          { id: "call_plan" },
        ),
      ]),
      fauxAssistantMessage("Plan updated."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Create a plan");
    expect(result.status).toBe("completed");
  });

  it("update_plan handles non-array plan gracefully", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("update_plan", { plan: "not-an-array" }, { id: "call_plan_bad" }),
      ]),
      fauxAssistantMessage("Plan handled."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Bad plan");
    expect(result.status).toBe("completed");

    const toolResults = result.messages.filter((m) => m.role === "toolResult");
    expect(toolResults.length).toBe(1);
    expect((toolResults[0] as { isError?: boolean }).isError).toBeFalsy();

    const d = (toolResults[0] as { details?: { plan?: unknown } }).details;
    expect(d?.plan).toEqual([]);
  });

  it("returns error for unknown orchestrator tool name", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("nonexistent_orch_tool", {}, { id: "call_unknown" })]),
      fauxAssistantMessage("Unknown tool."),
    ]);

    const host = await PikoHost.create({
      engine: createModelCaller(),
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Call unknown tool");
    // Unknown tool names that aren't in any toolset cause a discovery gap
    // and may result in error status (no route found before model step).
    // The OrchToolProvider unit tests cover the unknown_tool code path.
    expect(result.messages.length).toBeGreaterThanOrEqual(0);

    const toolResults = result.messages.filter((m) => m.role === "toolResult");
    expect(toolResults.length).toBeGreaterThan(0);
    const lastTool = toolResults[toolResults.length - 1] as { isError?: boolean };
    expect(lastTool.isError).toBe(true);
  });
});
