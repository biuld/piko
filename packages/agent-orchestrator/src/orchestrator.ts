import type {
  AgentOrchestrator as AgentOrchestratorInterface,
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentTaskState,
  AgentWatch,
  AgentWatchId,
  AgentWatchState,
  EngineToolSet,
  LockMode,
  LockState,
  OrchestratorEvent,
  OrchestratorEventEnvelope,
  OrchestratorEventListener,
  OrchestratorGraph,
  OrchestratorState,
  WakeReason,
} from "piko-engine-protocol";
import { reduceOrchestratorEvent } from "piko-engine-protocol";
import { renderGraph } from "./graph.js";
import { v4Id } from "./id.js";

let orchestratorCounter = 0;

export function createOrchestratorState(runId?: string): OrchestratorState {
  return {
    runId: runId ?? `orch-${Date.now()}-${orchestratorCounter++}`,
    status: "idle",
    toolSets: {},
    agents: {},
    tasks: {},
    watches: {},
    locks: {},
    approvals: {},
    artifacts: {},
  };
}

export class AgentOrchestrator implements AgentOrchestratorInterface {
  private _state: OrchestratorState;
  private _events: OrchestratorEventEnvelope[] = [];
  private _listeners: Set<OrchestratorEventListener> = new Set();
  private _tickTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(runId?: string) {
    this._state = createOrchestratorState(runId);
  }

  // ---- Lifecycle ----

  start(): void {
    if (this._state.status === "running") return;
    this._emit({ type: "orchestrator_started", runId: this._state.runId });
  }

  async stop(): Promise<void> {
    if (this._state.status !== "running") return;
    if (this._tickTimer) {
      clearTimeout(this._tickTimer);
      this._tickTimer = null;
    }
    this._emit({ type: "orchestrator_stopped", runId: this._state.runId, reason: "manual" });
  }

  // ---- ToolSet management ----

  registerToolSet(toolSet: EngineToolSet): void {
    const existing = this._state.toolSets[toolSet.id];
    if (existing) {
      // Merge: replace if re-registered
    }
    this._state.toolSets = { ...this._state.toolSets, [toolSet.id]: toolSet };
    this._emit({ type: "toolset_registered", toolSetId: toolSet.id, name: toolSet.name });
  }

  unregisterToolSet(toolSetId: string): void {
    delete (this._state as { toolSets: Record<string, EngineToolSet> }).toolSets[toolSetId];
    this._state = { ...this._state };
  }

  // ---- Agent management ----

  registerAgent(spec: AgentSpec): void {
    // Validate toolSetIds
    for (const tsId of spec.toolSetIds) {
      if (!this._state.toolSets[tsId]) {
        throw new Error(
          `Agent "${spec.id}" references unknown ToolSet "${tsId}". Register the ToolSet first.`,
        );
      }
    }

    const runtimeState: AgentRuntimeState = {
      id: spec.id,
      spec,
      status: "idle",
      inbox: [],
      transcript: [],
    };

    this._state.agents = { ...this._state.agents, [spec.id]: runtimeState };
    this._emit({
      type: "agent_registered",
      agentId: spec.id,
      name: spec.name,
      role: spec.role,
      toolSetIds: spec.toolSetIds,
    });
  }

  unregisterAgent(agentId: string): void {
    delete (this._state as { agents: Record<string, AgentRuntimeState> }).agents[agentId];
    this._state = { ...this._state };
    this._emit({ type: "agent_unregistered", agentId });
  }

  // ---- Task management ----

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    const agent = this._state.agents[task.targetAgentId];
    if (!agent) {
      throw new Error(`Agent not found: ${task.targetAgentId}`);
    }

    const taskId = task.id ?? v4Id("task");
    const taskState: AgentTaskState = {
      id: taskId,
      targetAgentId: task.targetAgentId,
      prompt: task.prompt,
      source: task.source,
      status: "queued",
      priority: task.priority ?? 0,
      parentTaskId: task.parentTaskId,
    };

    this._state.tasks = { ...this._state.tasks, [taskId]: taskState };

    // Add to agent inbox
    const agentState = this._state.agents[task.targetAgentId];
    this._state.agents = {
      ...this._state.agents,
      [task.targetAgentId]: {
        ...agentState,
        inbox: [...agentState.inbox, taskId],
      },
    };

    this._emit({ type: "task_enqueued", task: taskState });

    // Try to schedule immediately
    await this.tick();

    return taskId;
  }

  async wake(agentId: string, reason: WakeReason): Promise<void> {
    const agent = this._state.agents[agentId];
    if (!agent) return;

    this._state.agents = {
      ...this._state.agents,
      [agentId]: { ...agent, lastWakeReason: reason },
    };

    await this.tick();
  }

  async tick(_now?: number): Promise<void> {
    if (this._state.status !== "running") return;

    // Find runnable agents (have queued tasks + are idle)
    const runnableAgents = Object.values(this._state.agents).filter(
      (a) => a.inbox.length > 0 && a.status === "idle",
    );

    // Sort by priority
    runnableAgents.sort((_a, _b) => {
      // Get top task priorities
      const aTask = this._state.tasks[_a.inbox[0]];
      const bTask = this._state.tasks[_b.inbox[0]];
      return (bTask?.priority ?? 0) - (aTask?.priority ?? 0);
    });

    for (const agent of runnableAgents) {
      // Check concurrency
      const spec = agent.spec;
      const concurrency = spec.concurrency;

      if (concurrency?.requiresWriteLock) {
        const writeLock = Object.values(this._state.locks).find(
          (l) => l.resource === "workspace" && l.mode === "write" && l.holderAgentId,
        );
        if (writeLock) {
          this._emit({
            type: "scheduler_decision",
            decision: {
              kind: "deferred",
              agentId: agent.id,
              reason: "lock_unavailable",
            },
          });
          continue;
        }
      }

      if (concurrency?.maxConcurrentTasks !== undefined && concurrency.maxConcurrentTasks <= 0) {
        this._emit({
          type: "scheduler_decision",
          decision: {
            kind: "deferred",
            agentId: agent.id,
            reason: "agent_busy",
          },
        });
        continue;
      }

      // Dequeue task from inbox
      const taskId = agent.inbox[0];
      const task = this._state.tasks[taskId];
      if (!task) continue;

      // Start the task
      this._state.tasks = {
        ...this._state.tasks,
        [taskId]: { ...task, status: "running" },
      };
      this._state.agents = {
        ...this._state.agents,
        [agent.id]: {
          ...agent,
          status: "running",
          activeTaskId: taskId,
          inbox: agent.inbox.slice(1),
        },
      };

      this._emit({ type: "agent_status_changed", agentId: agent.id, from: "idle", to: "running" });
      this._emit({ type: "task_started", taskId, agentId: agent.id });

      this._emit({
        type: "scheduler_decision",
        decision: { kind: "started", agentId: agent.id, taskId },
      });

      // Only start one agent per tick (host will call tick again)
      break;
    }

    // Emit skipped for non-runnable agents
    for (const agent of Object.values(this._state.agents)) {
      if (agent.inbox.length > 0 && agent.status !== "idle") {
        this._emit({
          type: "scheduler_decision",
          decision: {
            kind: "skipped",
            agentId: agent.id,
            reason: "agent_busy",
          },
        });
      }
    }
    if (runnableAgents.length === 0) {
      const hasQueued = Object.values(this._state.agents).some((a) => a.inbox.length > 0);
      if (hasQueued) {
        this._emit({
          type: "scheduler_decision",
          decision: { kind: "skipped", reason: "no_tasks" },
        });
      }
    }
  }

  // ---- Task lifecycle (called by Host after engine step) ----

  completeTask(taskId: AgentTaskId, result: AgentTaskResult): void {
    const task = this._state.tasks[taskId];
    if (!task) return;

    this._state.tasks = {
      ...this._state.tasks,
      [taskId]: { ...task, status: "completed", result },
    };

    if (task.parentTaskId) {
      // Enqueue wake for parent agent
      const parentTask = this._state.tasks[task.parentTaskId];
      if (parentTask) {
        this._emit({
          type: "watch_triggered",
          watchId: `dep-${task.parentTaskId}`,
          agentId: parentTask.targetAgentId,
          reason: { kind: "subagent_result", fromAgentId: task.targetAgentId, taskId },
        });
      }
    }
    this._emit({ type: "task_completed", taskId, agentId: task.targetAgentId, result });

    // Reset agent to idle
    const agent = this._state.agents[task.targetAgentId];
    if (agent && agent.activeTaskId === taskId) {
      this._state.agents = {
        ...this._state.agents,
        [task.targetAgentId]: {
          ...agent,
          status: "idle",
          activeTaskId: undefined,
        },
      };
      this._emit({
        type: "agent_status_changed",
        agentId: task.targetAgentId,
        from: "running",
        to: "idle",
      });
    }
  }

  failTask(taskId: AgentTaskId, error: string): void {
    const task = this._state.tasks[taskId];
    if (!task) return;

    this._state.tasks = {
      ...this._state.tasks,
      [taskId]: { ...task, status: "failed", error },
    };
    this._emit({ type: "task_failed", taskId, agentId: task.targetAgentId, error });

    const agent = this._state.agents[task.targetAgentId];
    if (agent && agent.activeTaskId === taskId) {
      this._state.agents = {
        ...this._state.agents,
        [task.targetAgentId]: { ...agent, status: "failed", activeTaskId: undefined },
      };
      this._emit({
        type: "agent_status_changed",
        agentId: task.targetAgentId,
        from: "running",
        to: "failed",
      });
    }
  }

  blockTask(taskId: AgentTaskId, reason: string): void {
    const task = this._state.tasks[taskId];
    if (!task) return;
    this._state.tasks = {
      ...this._state.tasks,
      [taskId]: { ...task, status: "blocked" },
    };
    this._emit({ type: "task_blocked", taskId, agentId: task.targetAgentId, reason });
  }

  // ---- Locks ----

  requestLock(agentId: string, taskId: string, resource: string, mode: LockMode): boolean {
    const lockId = `${resource}-lock`;
    let lock = this._state.locks[lockId];

    if (!lock) {
      lock = {
        id: lockId,
        resource,
        mode,
        queue: [],
      };
    }

    this._emit({
      type: "lock_requested",
      lockId,
      agentId,
      taskId,
      resource,
      mode,
    });

    if (!lock.holderAgentId) {
      // Acquire immediately
      lock = { ...lock, holderAgentId: agentId, holderTaskId: taskId, mode };
      this._state.locks = { ...this._state.locks, [lockId]: lock };
      this._emit({
        type: "lock_acquired",
        lockId,
        agentId,
        taskId,
        resource,
        mode,
      });
      return true;
    }

    if (lock.holderAgentId === agentId) {
      // Upgrade/downgrade
      lock = { ...lock, mode, holderTaskId: taskId };
      this._state.locks = { ...this._state.locks, [lockId]: lock };
      return true;
    }

    if (mode === "read" && lock.mode === "read") {
      // Multiple readers ok
      return true;
    }

    // Queue the request
    lock = {
      ...lock,
      queue: [...lock.queue, { agentId, taskId, mode }],
    };
    this._state.locks = { ...this._state.locks, [lockId]: lock };
    return false;
  }

  releaseLock(agentId: string, taskId: string, resource: string): void {
    const lockId = `${resource}-lock`;
    const lock = this._state.locks[lockId];
    if (!lock || lock.holderAgentId !== agentId) return;

    this._emit({
      type: "lock_released",
      lockId,
      agentId,
      taskId,
      resource,
    });

    // Promote next waiter
    if (lock.queue.length > 0) {
      const next = lock.queue.shift()!;
      const newLock: LockState = {
        ...lock,
        holderAgentId: next.agentId,
        holderTaskId: next.taskId,
        mode: next.mode,
        queue: [...lock.queue],
      };
      this._state.locks = { ...this._state.locks, [lockId]: newLock };
      this._emit({
        type: "lock_acquired",
        lockId,
        agentId: next.agentId,
        taskId: next.taskId,
        resource,
        mode: next.mode,
      });
    } else {
      const releasedLock: LockState = {
        ...lock,
        holderAgentId: undefined,
        holderTaskId: undefined,
        queue: [],
      };
      this._state.locks = { ...this._state.locks, [lockId]: releasedLock };
    }
  }

  // ---- Watches ----

  registerWatch(watch: AgentWatch): AgentWatchId {
    const id = watch.id ?? v4Id("watch");
    const state: AgentWatchState = {
      id,
      agentId: watch.agentId,
      kind: watch.kind,
      active: true,
    };
    this._state.watches = { ...this._state.watches, [id]: state };
    this._emit({ type: "watch_registered", watchId: id, agentId: watch.agentId, kind: watch.kind });

    // For interval watches, start ticking
    if (watch.kind === "interval") {
      this._startIntervalWatch(id, watch);
    }

    return id;
  }

  unregisterWatch(watchId: AgentWatchId): void {
    const watch = this._state.watches[watchId];
    if (!watch) return;

    this._state.watches = {
      ...this._state.watches,
      [watchId]: { ...watch, active: false },
    };
    this._emit({ type: "watch_unregistered", watchId });
  }

  private _startIntervalWatch(
    watchId: string,
    watch: Extract<AgentWatch, { kind: "interval" }>,
  ): void {
    const interval = setInterval(() => {
      const ws = this._state.watches[watchId];
      if (!ws?.active) {
        clearInterval(interval);
        return;
      }
      this._emit({
        type: "watch_triggered",
        watchId,
        agentId: watch.agentId,
        reason: { kind: "timer", watchId },
      });
      this.dispatch({
        targetAgentId: watch.agentId,
        prompt: watch.prompt,
        source: { kind: "timer", watchId },
      }).catch(() => {});
    }, watch.everyMs);
  }

  // ---- Subscriptions ----

  subscribe(listener: OrchestratorEventListener): () => void {
    this._listeners.add(listener);
    return () => {
      this._listeners.delete(listener);
    };
  }

  // ---- State access ----

  snapshot(): OrchestratorState {
    return this._state;
  }

  dumpEvents(): OrchestratorEventEnvelope[] {
    return [...this._events];
  }

  renderGraph(): OrchestratorGraph {
    return renderGraph(this._state);
  }

  // ---- Internal ----

  private _emit(event: OrchestratorEvent): void {
    const envelope: OrchestratorEventEnvelope = {
      meta: {
        eventId: v4Id("evt"),
        timestamp: Date.now(),
        orchestratorRunId: this._state.runId,
      },
      event,
    };

    this._events.push(envelope);
    this._state = reduceOrchestratorEvent(this._state, envelope);

    // Notify listeners
    for (const listener of this._listeners) {
      try {
        listener(envelope, this._state);
      } catch {
        // Listener errors must not break the orchestrator
      }
    }
  }
}
