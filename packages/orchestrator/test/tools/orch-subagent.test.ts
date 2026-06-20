// ---- Orchestrator subagent tests (pure orchestrator, no host-runtime) ----
// Verifies delegate_to_agent / join_subtask / get_orchestrator_state / update_plan
// via tool calls. OrchToolProvider is auto-registered by the Orchestrator constructor.

import { describe, expect, it } from "bun:test";
import type { ToolSet } from "piko-orchestrator-protocol";
import { Orchestrator } from "../../src/orchestrator/index.js";
import type { FauxStepSpec } from "../helpers/index.js";
import { createFauxModelExecutor } from "../helpers/index.js";

// ---- ToolSet with orch control tools ----

const BUILTIN_TOOLSET: ToolSet = {
  id: "builtin",
  name: "Built-in",
  tools: [
    { kind: "orchestrator_control", action: "delegate_to_agent" },
    { kind: "orchestrator_control", action: "join_subtask" },
    { kind: "orchestrator_control", action: "get_orchestrator_state" },
    { kind: "orchestrator_control", action: "update_plan" },
  ],
};

function setupOrch(steps: FauxStepSpec[]): Orchestrator {
  const orch = new Orchestrator(createFauxModelExecutor({ steps }));
  orch.registerToolSet(BUILTIN_TOOLSET);
  return orch;
}

// =========================================================================

describe("Orchestrator subagent tools (pure)", () => {
  // ---- delegate_to_agent (call mode) ----

  it("delegate_to_agent call mode — coordinator delegates, subagent runs, result flows back", async () => {
    const orch = setupOrch([
      {
        toolCalls: [
          {
            id: "d1",
            name: "delegate_to_agent",
            arguments: { agentId: "w", prompt: "Build", mode: "call" },
          },
        ],
        status: "continue",
      },
      { content: "Built.", status: "completed" },
      { content: "Delegation done.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });
    orch.registerAgent({
      id: "w",
      name: "Worker",
      role: "test",
      systemPrompt: "Worker.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Delegate to worker");
    expect(result.status).toBe("completed");
    expect(result.messages.some((m) => m.role === "toolResult")).toBe(true);
  });

  // ---- delegate_to_agent (detach mode) ----

  it("delegate_to_agent detach mode — returns taskId handle", async () => {
    const orch = setupOrch([
      {
        toolCalls: [
          {
            id: "d1",
            name: "delegate_to_agent",
            arguments: { agentId: "w", prompt: "Bg", mode: "detach" },
          },
        ],
        status: "continue",
      },
      { content: "Delegated.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });
    orch.registerAgent({
      id: "w",
      name: "Worker",
      role: "test",
      systemPrompt: "Worker.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Detach work");
    expect(result.status).toBe("completed");

    const tr = result.messages.find((m) => m.role === "toolResult") as
      | { details?: unknown }
      | undefined;
    const d = tr?.details as { mode?: string; taskId?: string } | undefined;
    expect(d?.mode).toBe("detach");
    expect(d?.taskId).toBeDefined();
  });

  // ---- get_orchestrator_state ----

  it("get_orchestrator_state returns snapshot", async () => {
    const orch = setupOrch([
      {
        toolCalls: [
          { id: "s1", name: "get_orchestrator_state", arguments: { format: "snapshot" } },
        ],
        status: "continue",
      },
      { content: "Got state.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Show state");
    expect(result.status).toBe("completed");
    expect(result.messages.some((m) => m.role === "toolResult")).toBe(true);
  });

  // ---- update_plan ----

  it("update_plan executes successfully", async () => {
    const orch = setupOrch([
      {
        toolCalls: [{ id: "p1", name: "update_plan", arguments: { plan: [{ step: 1 }] } }],
        status: "continue",
      },
      { content: "Plan set.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Set plan");
    expect(result.status).toBe("completed");
  });

  // ---- Error: self-delegation ----

  it("delegate_to_agent rejects self-delegation", async () => {
    const orch = setupOrch([
      {
        toolCalls: [
          { id: "d1", name: "delegate_to_agent", arguments: { agentId: "main", prompt: "Do it" } },
        ],
        status: "continue",
      },
      { content: "Can't self-delegate.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Self-delegate");
    expect(result.status).toBe("completed");

    const tr = result.messages.find((m) => m.role === "toolResult") as
      | { isError?: boolean }
      | undefined;
    expect(tr?.isError).toBe(true);
  });

  // ---- Error: unknown agent ----

  it("delegate_to_agent rejects unknown target agent", async () => {
    const orch = setupOrch([
      {
        toolCalls: [
          { id: "d1", name: "delegate_to_agent", arguments: { agentId: "ghost", prompt: "Work" } },
        ],
        status: "continue",
      },
      { content: "Ghost not found.", status: "completed" },
    ]);

    orch.registerAgent({
      id: "main",
      name: "main",
      role: "test",
      systemPrompt: "Main.",
      toolSetIds: ["builtin"],
    });

    const result = await orch.run("Delegate to ghost");
    expect(result.status).toBe("completed");

    const tr = result.messages.find((m) => m.role === "toolResult") as
      | { isError?: boolean }
      | undefined;
    expect(tr?.isError).toBe(true);
  });
});
