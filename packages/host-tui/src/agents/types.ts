// ============================================================================
// Agent panel view model — presentation-ready state supplied by Host selectors.
// ============================================================================

export type AgentPanelMode = "collapsed" | "expanded";

export type AgentPanelStatus = "idle" | "running" | "failed" | "stopped";

export type AgentPlanStepStatus = "pending" | "in_progress" | "completed";

export interface AgentPlanStepViewModel {
  id?: string;
  step: string;
  status: AgentPlanStepStatus;
}

export interface AgentTaskViewModel {
  id: string;
  title: string;
  plan: AgentPlanStepViewModel[];
  error?: string;
}

export type AgentQueueKind = "steering" | "follow_up" | "next_turn";

export interface AgentQueueItemViewModel {
  id?: string;
  kind: AgentQueueKind;
  preview: string;
}

export interface AgentPanelViewModel {
  id: string;
  name: string;
  status: AgentPanelStatus;
  activeTask?: AgentTaskViewModel;
  queue?: AgentQueueItemViewModel[];
}

/** Selection intent emitted by AgentPanel; the parent decides how TUI state changes. */
export interface AgentPanelSelectEvent {
  type: "agent_selected";
  agentId: string;
}

export type AgentPanelRowTone = "normal" | "muted" | "accent" | "success" | "error";

export interface AgentPanelRow {
  key: string;
  kind: "agent" | "plan" | "queue" | "error";
  icon: string;
  /** Animate the agent activity marker instead of rendering the static icon. */
  spinner?: boolean;
  name?: string;
  progress?: string;
  detail?: string;
  queue?: string;
  tone: AgentPanelRowTone;
  indent: number;
}
