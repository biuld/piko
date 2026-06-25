import type { HostEvent, Message } from "piko-orch-protocol";
import type { SessionManager, SessionPersistenceOverview } from "../../session/index.js";

export class HostPersistence {
  private taskSessions = new Map<string, string>();
  private queue: Promise<void> = Promise.resolve();

  constructor(
    private readonly getSessionManager: () => SessionManager,
    private readonly getModelId: () => string,
  ) {}

  enqueueEvent(event: HostEvent): void {
    this.queue = this.queue.then(() => this.persistEvent(event)).catch(() => {});
  }

  async flush(): Promise<void> {
    await this.queue;
  }

  async refreshSession(): Promise<SessionPersistenceOverview> {
    const sessionManager = this.getSessionManager();
    const overview = await sessionManager.loadPersistenceOverview();
    this.taskSessions = new Map(
      overview.tasks
        .filter((task) => task.agentId !== "main")
        .map((task) => [task.taskId, task.agentSessionId]),
    );
    return overview;
  }

  async loadAgentHistory(agentId: string): Promise<Message[]> {
    if (agentId === "main") {
      return this.getSessionManager().loadMessages();
    }
    return [];
  }

  async saveAgentMessages(agentId: string, messages: Message[]): Promise<void> {
    const sessionManager = this.getSessionManager();
    if (agentId === "main") {
      await sessionManager.saveMessages(this.getModelId(), messages);
      return;
    }

    const task = (await sessionManager.loadTaskTree())
      .filter((record) => record.agentId === agentId)
      .at(-1);
    if (!task) return;
    const agentSession = await sessionManager.openAgentSession(task.agentSessionId);
    await agentSession?.saveMessages(this.getModelId(), messages);
  }

  private async persistEvent(event: HostEvent): Promise<void> {
    const sessionManager = this.getSessionManager();

    switch (event.type) {
      case "task_created": {
        const task = event.task;
        if (task.targetAgentId === "main") return;
        const agentSession = await sessionManager.createAgentSession(task.targetAgentId, {
          displayName: task.targetAgentId,
        });
        this.taskSessions.set(task.id, agentSession.getSessionId());
        await sessionManager.appendAgentTask({
          taskId: task.id,
          agentId: task.targetAgentId,
          agentSessionId: agentSession.getSessionId(),
          parentTaskId: task.parentTaskId,
          sourceAgentId: task.source.type === "agent" ? task.source.agentId : undefined,
          sourceTaskId: task.source.type === "agent" ? task.source.taskId : undefined,
          status: "queued",
        });
        await this.refreshSession();
        return;
      }
      case "task_started": {
        if (!this.taskSessions.has(event.taskId)) return;
        await sessionManager.updateAgentTaskStatus(event.taskId, "running");
        return;
      }
      case "task_completed": {
        if (!this.taskSessions.has(event.taskId)) return;
        await sessionManager.updateAgentTaskStatus(event.taskId, "completed", {
          summary: event.result.summary,
        });
        return;
      }
      case "task_failed": {
        if (!this.taskSessions.has(event.taskId)) return;
        await sessionManager.updateAgentTaskStatus(event.taskId, "failed", {
          error: event.error,
        });
        return;
      }
      case "task_transcript_committed": {
        const agentSessionId = this.taskSessions.get(event.taskId);
        if (!agentSessionId || event.agentId === "main") return;
        const agentSession = await sessionManager.openAgentSession(agentSessionId);
        if (!agentSession) return;
        await agentSession.saveMessages(this.getModelId(), event.messages);
        return;
      }
      case "tool_start": {
        const agentSessionId = this.taskSessions.get(event.taskId);
        if (!agentSessionId) return;
        await sessionManager.appendAgentRuntimeEvent({
          taskId: event.taskId,
          agentId: event.agentId,
          agentSessionId,
          event: {
            type: "tool_started",
            callId: event.id,
            name: event.name,
            args: event.args,
          },
        });
        return;
      }
      case "tool_end": {
        const agentSessionId = this.taskSessions.get(event.taskId);
        if (!agentSessionId) return;
        await sessionManager.appendAgentRuntimeEvent({
          taskId: event.taskId,
          agentId: event.agentId,
          agentSessionId,
          event: {
            type: "tool_finished",
            callId: event.id,
            name: event.name,
            result: event.result,
            isError: event.isError,
          },
        });
        return;
      }
      default:
        return;
    }
  }
}
