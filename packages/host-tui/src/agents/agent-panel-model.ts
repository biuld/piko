import type {
  AgentPanelMode,
  AgentPanelRow,
  AgentPanelStatus,
  AgentPanelViewModel,
  AgentPlanStepStatus,
  AgentPlanStepViewModel,
} from "./types.js";

/** Build semantic rows without coupling agent state to OpenTUI rendering. */
export function buildAgentPanelRows(
  agent: AgentPanelViewModel,
  mode: AgentPanelMode,
): AgentPanelRow[] {
  const task = agent.activeTask;
  if (!task) {
    return [
      {
        key: `agent:${agent.id}`,
        kind: "agent",
        icon: agentStatusIcon(agent.status),
        spinner: agent.status === "running",
        name: agent.name,
        detail: agentStatusLabel(agent.status),
        queue: queueSummary(agent.queue?.length),
        tone: agentStatusTone(agent.status),
        indent: 0,
      },
    ];
  }

  if (mode === "collapsed") {
    if (agent.status === "failed" && task.error) {
      return [
        {
          key: `agent:${agent.id}`,
          kind: "agent",
          icon: agentStatusIcon(agent.status),
          spinner: false,
          name: agent.name,
          detail: task.error,
          queue: queueSummary(agent.queue?.length),
          tone: "error",
          indent: 0,
        },
      ];
    }
    const summary = selectPlanSummary(task.plan);
    return [
      {
        key: `agent:${agent.id}`,
        kind: "agent",
        icon: agentStatusIcon(agent.status),
        spinner: agent.status === "running",
        name: agent.name,
        progress: summary?.position,
        detail: summary?.label ?? task.title,
        queue: queueSummary(agent.queue?.length),
        tone: agentStatusTone(agent.status),
        indent: 0,
      },
    ];
  }

  const rows: AgentPanelRow[] = [
    {
      key: `agent:${agent.id}`,
      kind: "agent",
      icon: agentStatusIcon(agent.status),
      spinner: agent.status === "running",
      name: agent.name,
      detail: task.title,
      queue: queueSummary(agent.queue?.length),
      tone: agentStatusTone(agent.status),
      indent: 0,
    },
  ];

  if (agent.status === "failed" && task.error) {
    rows.push({
      key: `task:${task.id}:error`,
      kind: "error",
      icon: "!",
      progress: "error",
      detail: task.error,
      tone: "error",
      indent: 1,
    });
  }

  for (const [index, step] of task.plan.entries()) {
    rows.push({
      key: step.id ?? `task:${task.id}:step:${index}`,
      kind: "plan",
      icon: planStepIcon(step.status),
      progress: `${index + 1}/${task.plan.length}`,
      detail: step.step,
      tone: planStepTone(step.status),
      indent: 1,
    });
  }

  for (const [index, item] of (agent.queue ?? []).entries()) {
    rows.push({
      key: item.id ?? `agent:${agent.id}:queue:${index}`,
      kind: "queue",
      icon: "↳",
      progress: item.kind.replace("_", "-"),
      detail: item.preview,
      tone: "muted",
      indent: 1,
    });
  }

  return rows;
}

function queueSummary(count: number | undefined): string | undefined {
  return count ? `${count} queued` : undefined;
}

export function selectPlanSummary(
  plan: AgentPlanStepViewModel[],
): { position: string; label: string; status: AgentPlanStepStatus } | undefined {
  if (plan.length === 0) return undefined;

  const activeIndex = plan.findIndex((step) => step.status === "in_progress");
  if (activeIndex >= 0) {
    return {
      position: `${activeIndex + 1}/${plan.length}`,
      label: plan[activeIndex].step,
      status: "in_progress",
    };
  }

  if (plan.every((step) => step.status === "completed")) {
    return { position: `${plan.length}/${plan.length}`, label: "Completed", status: "completed" };
  }

  const nextIndex = plan.findIndex((step) => step.status === "pending");
  return {
    position: `0/${plan.length}`,
    label: nextIndex >= 0 ? plan[nextIndex].step : "Waiting",
    status: "pending",
  };
}

function agentStatusIcon(status: AgentPanelStatus): string {
  if (status === "running") return "●";
  if (status === "failed") return "!";
  if (status === "stopped") return "■";
  return "○";
}

function agentStatusLabel(status: AgentPanelStatus): string {
  if (status === "running") return "Running";
  if (status === "failed") return "Failed";
  if (status === "stopped") return "Stopped";
  return "Idle";
}

function agentStatusTone(status: AgentPanelStatus): AgentPanelRow["tone"] {
  if (status === "running") return "accent";
  if (status === "failed") return "error";
  return "muted";
}

function planStepIcon(status: AgentPlanStepStatus): string {
  if (status === "completed") return "✓";
  if (status === "in_progress") return "●";
  return "○";
}

function planStepTone(status: AgentPlanStepStatus): AgentPanelRow["tone"] {
  if (status === "completed") return "success";
  if (status === "in_progress") return "accent";
  return "muted";
}
