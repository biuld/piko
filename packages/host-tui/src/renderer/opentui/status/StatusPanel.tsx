import type { OrchState } from "piko-orch-protocol";
import { For, Show } from "solid-js";
import type { AgentPanelViewModel, AgentPlanStepViewModel } from "../../../agents/types.js";
import { truncateToWidth } from "../../../layout/measure.js";
import { AgentPanel } from "../agents/AgentPanel.js";
import { useTheme } from "../theme-context.js";
import type { StatusContract } from "./types.js";

export interface StatusPanelProps {
  status: StatusContract;
  snapshot?: OrchState;
  currentAgentId: string;
  viewedAgentId: string;
  expandedAgentId?: string;
  width: number;
  spinnerFrame?: number;
  onViewedAgentChange: (agentId: string) => void;
  onToggleExpand: () => void;
}

/** Status composition root. AgentPanel owns presentation; this component owns composition only. */
export function StatusPanel(props: StatusPanelProps) {
  const theme = useTheme();
  const agents = () => projectAgents(props.snapshot, props.status, props.currentAgentId);

  return (
    <box
      flexDirection="column"
      flexShrink={0}
      overflow="hidden"
      border={["top"]}
      borderColor={theme.color("border.muted")}
    >
      <For each={agents()}>
        {(agent) => (
          <AgentPanel
            agent={agent}
            mode={props.expandedAgentId === agent.id ? "expanded" : "collapsed"}
            width={props.width}
            selected={props.viewedAgentId === agent.id}
            spinnerFrame={props.spinnerFrame}
            onSelect={({ agentId }) => {
              if (props.viewedAgentId === agentId) {
                props.onToggleExpand();
              } else {
                props.onViewedAgentChange(agentId);
              }
            }}
          />
        )}
      </For>
      <Show when={props.status.notification}>
        {(notification) => {
          const tone = () => notificationTone(notification().severity);
          const contentWidth = () => Math.max(0, props.width - 4);
          return (
            <box
              height={1}
              flexDirection="row"
              width={props.width}
              paddingLeft={1}
              overflow="hidden"
            >
              <box width={1} />
              <box width={1} overflow="hidden">
                <text fg={theme.color(tone())}>│</text>
              </box>
              <box width={1} />
              <box flexShrink={1} overflow="hidden">
                <text fg={theme.color(tone())}>
                  {truncateToWidth(notification().message, contentWidth())}
                </text>
              </box>
            </box>
          );
        }}
      </Show>
    </box>
  );
}

export function projectAgents(
  snapshot: OrchState | undefined,
  status: StatusContract,
  currentAgentId: string,
): AgentPanelViewModel[] {
  if (!snapshot || Object.keys(snapshot.agents).length === 0) {
    return [fallbackAgent(status, currentAgentId)];
  }
  return Object.values(snapshot.agents).map((agent) => {
    const task = agent.activeTaskId ? snapshot.tasks[agent.activeTaskId] : undefined;
    return {
      id: agent.id,
      name: (agent.spec.name || agent.id).toLowerCase(),
      status: agent.status,
      ...(task
        ? {
            activeTask: {
              id: task.id,
              title: task.prompt,
              plan: parsePlan(task.plan),
              error: task.error,
            },
          }
        : {}),
      ...(agent.id === currentAgentId ? { queue: projectQueue(status) } : {}),
    };
  });
}

function fallbackAgent(status: StatusContract, agentId: string): AgentPanelViewModel {
  return {
    id: agentId,
    name: agentId.toLowerCase(),
    status: status.state === "working" || status.state === "compacting" ? "running" : "idle",
    queue: projectQueue(status),
  };
}

function parsePlan(plan: unknown[] | undefined): AgentPlanStepViewModel[] {
  if (!plan) return [];
  return plan.flatMap((value) => {
    if (!value || typeof value !== "object") return [];
    const step = value as Record<string, unknown>;
    if (typeof step.step !== "string") return [];
    if (!isPlanStatus(step.status)) return [];
    return [
      {
        step: step.step,
        status: step.status,
        ...(typeof step.id === "string" ? { id: step.id } : {}),
      },
    ];
  });
}

function isPlanStatus(value: unknown): value is AgentPlanStepViewModel["status"] {
  return value === "pending" || value === "in_progress" || value === "completed";
}

function projectQueue(status: StatusContract): AgentPanelViewModel["queue"] {
  const queue = status.queue;
  if (!queue) return undefined;
  return [
    ...queue.steering.map((item) => ({ kind: "steering" as const, preview: item.preview })),
    ...queue.followUp.map((item) => ({ kind: "follow_up" as const, preview: item.preview })),
    ...(queue.nextTurnCount > 0
      ? [{ kind: "next_turn" as const, preview: `${queue.nextTurnCount} next-turn messages` }]
      : []),
  ];
}

function notificationTone(
  severity: NonNullable<StatusContract["notification"]>["severity"],
): string {
  if (severity === "error") return "text.error";
  if (severity === "warning") return "text.warning";
  if (severity === "success") return "text.success";
  return "text.accent";
}
