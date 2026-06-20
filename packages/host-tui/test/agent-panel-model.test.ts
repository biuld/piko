import { describe, expect, it } from "bun:test";
import { getAgentPanelColumns } from "../src/agents/agent-panel-layout.js";
import { buildAgentPanelRows, selectPlanSummary } from "../src/agents/agent-panel-model.js";
import type { AgentPanelViewModel } from "../src/agents/types.js";

function runningAgent(): AgentPanelViewModel {
  return {
    id: "main",
    name: "main",
    status: "running",
    activeTask: {
      id: "task-1",
      title: "Redesign agent activity",
      plan: [
        { id: "inspect", step: "Inspect architecture", status: "completed" },
        { id: "design", step: "Design AgentPanel", status: "in_progress" },
        { id: "verify", step: "Verify behavior", status: "pending" },
      ],
    },
  };
}

describe("AgentPanel model", () => {
  it("renders one current-step row when collapsed", () => {
    expect(buildAgentPanelRows(runningAgent(), "collapsed")).toEqual([
      {
        key: "agent:main",
        kind: "agent",
        icon: "●",
        spinner: true,
        name: "main",
        progress: "2/3",
        detail: "Design AgentPanel",
        queue: undefined,
        tone: "accent",
      },
    ]);
  });

  it("renders the task and every plan step when expanded", () => {
    const rows = buildAgentPanelRows(runningAgent(), "expanded");
    expect(rows).toHaveLength(4);
    expect(rows[0]).toMatchObject({ name: "main" });
    expect(rows[0]).not.toHaveProperty("detail");
    expect(rows.slice(1)).toEqual([
      {
        key: "inspect",
        kind: "plan",
        icon: "✓",
        progress: "1/3",
        detail: "Inspect architecture",
        tone: "success",
      },
      {
        key: "design",
        kind: "plan",
        icon: "●",
        progress: "2/3",
        detail: "Design AgentPanel",
        tone: "accent",
      },
      {
        key: "verify",
        kind: "plan",
        icon: "○",
        progress: "3/3",
        detail: "Verify behavior",
        tone: "muted",
      },
    ]);
  });

  it("reports completed and not-started plans deterministically", () => {
    expect(
      selectPlanSummary([
        { step: "One", status: "completed" },
        { step: "Two", status: "completed" },
      ]),
    ).toEqual({ position: "2/2", label: "Completed", status: "completed" });

    expect(
      selectPlanSummary([
        { step: "One", status: "pending" },
        { step: "Two", status: "pending" },
      ]),
    ).toEqual({ position: "0/2", label: "One", status: "pending" });
  });

  it("shows nothing in detail when no plan exists in collapsed mode", () => {
    const agent = runningAgent();
    agent.activeTask!.plan = [];
    const row = buildAgentPanelRows(agent, "collapsed")[0];
    expect(row.detail).toBeUndefined();
    expect(row.progress).toBeUndefined();
  });

  it("renders an idle agent without an active task and no detail placeholder", () => {
    const rows = buildAgentPanelRows(
      { id: "reviewer", name: "reviewer", status: "idle" },
      "collapsed",
    );
    expect(rows[0]).toMatchObject({
      key: "agent:reviewer",
      kind: "agent",
      icon: "○",
      spinner: false,
      name: "reviewer",
      tone: "muted",
    });
    expect(rows[0].detail).toBeUndefined();
  });

  it("prioritizes task errors in collapsed mode and retains plan context when expanded", () => {
    const agent = runningAgent();
    agent.status = "failed";
    agent.activeTask!.error = "Model request failed";

    expect(buildAgentPanelRows(agent, "collapsed")[0]).toMatchObject({
      icon: "!",
      spinner: false,
      detail: "Model request failed",
      tone: "error",
    });

    const expanded = buildAgentPanelRows(agent, "expanded");
    expect(expanded[1]).toMatchObject({
      progress: "error",
      detail: "Model request failed",
      tone: "error",
    });
    expect(expanded).toHaveLength(5);
  });

  it("keeps per-agent queue summary and expanded items in separate columns", () => {
    const agent = runningAgent();
    agent.queue = [{ id: "q1", kind: "follow_up", preview: "Run focused tests" }];

    expect(buildAgentPanelRows(agent, "collapsed")[0].queue).toBe("1 queued");
    expect(buildAgentPanelRows(agent, "expanded").at(-1)).toMatchObject({
      kind: "queue",
      progress: "follow-up",
      detail: "Run focused tests",
    });
  });

  it("allocates queue as its own column only when space permits", () => {
    const cols80 = getAgentPanelColumns(80);
    expect(cols80.marker).toBe(1);
    expect(cols80.gap).toBe(1);
    expect(cols80.queue).toBe(12);
    expect(getAgentPanelColumns(50).queue).toBe(0);
    const sum80 =
      cols80.marker + cols80.gap * 5 + cols80.name + cols80.progress + cols80.detail + cols80.queue;
    expect(sum80).toBe(80);
  });
});
