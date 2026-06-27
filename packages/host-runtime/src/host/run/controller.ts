import type { ModelStepEvent } from "../../models/executor.js";
import type { HostConfig } from "../../models/index.js";
import type { EventBus } from "../../orchd/event-bus.js";
import {
  EventStream,
  type HostEvent,
  type Message,
  type Orchestrator,
  type OrchWireEvent,
  startDebugSpan,
} from "../../orchd/protocol/index.js";
import type { SessionManager } from "../../session/index.js";
import type { McpServerManager } from "../../tools/mcp-provider.js";
import type { HostPersistence } from "../persistence/index.js";
import type { HostRunResult, StreamPromptOptions, StreamPromptResult } from "../shared/index.js";
import type { HostState } from "../state/index.js";
import type { AgentNameAssigner } from "./agent-name-assigner.js";
import { prepareOrchestratorRun } from "./prepare-orchestrator-run.js";

export class HostRunController {
  constructor(
    private readonly deps: {
      getOrchestrator: () => Orchestrator | undefined;
      getConfig: () => HostConfig;
      getSystemPrompt: () => string;
      getActiveToolNames: () => string[] | undefined;
      getMcpServers: () => Record<string, unknown> | undefined;
      getMcpManager: () => McpServerManager | undefined;
      setMcpManager: (manager: McpServerManager | undefined) => void;
      getSessionManager: () => SessionManager;
      agentNameAssigner: AgentNameAssigner;
      persistence: HostPersistence;
      state: HostState;
      eventBus: EventBus;
    },
  ) {}

  async run(prompt: string, signal?: AbortSignal, agentId = "main"): Promise<HostRunResult> {
    return await this.runCore(prompt, signal, undefined, agentId);
  }

  /** Legacy stream for skills/prompts — emits ModelStepEvent. */
  streamPrompt(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    const stream = new EventStream<ModelStepEvent, StreamPromptResult>();
    const agentId = options.agentId ?? "main";

    void this.runCore(
      prompt,
      signal,
      (event) => {
        switch (event.type) {
          case "token":
            stream.push({ type: "message_delta", messageId: "s", delta: event.text });
            break;
          case "thinking":
            stream.push({ type: "thinking_delta", messageId: "s", delta: event.text });
            break;
        }
      },
      agentId,
    )
      .then((result) => {
        stream.end({
          messages: result.messages,
          appendedMessages: result.messages,
          status: result.status,
          sessionId: result.sessionId,
          sessionFile: result.sessionFile,
          error: result.error,
        });
      })
      .catch((err) => {
        stream.push({ type: "error", message: err instanceof Error ? err.message : String(err) });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      });

    return stream;
  }

  /** Unified stream — emits HostEvent to EventBus subscribers. */
  streamPromptUnified(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<HostEvent, StreamPromptResult> {
    const stream = new EventStream<HostEvent, StreamPromptResult>();
    const agentId = options.agentId ?? "main";

    // Subscribe to EventBus for the duration of the run, filter by our agent
    const unsub = this.deps.eventBus.subscribe((event) => {
      // Filter: only forward events for our agent/task
      if ("agent_id" in event && event.agent_id !== agentId) return;
      stream.push(event);
    });

    void this.runCore(
      prompt,
      signal,
      () => {
        // The orchestrator callback is not needed — EventBus handles delivery.
        // We still pass a no-op because the signature requires it.
      },
      agentId,
    )
      .then((result) => {
        unsub();
        stream.end({
          messages: result.messages,
          appendedMessages: result.messages,
          status: result.status,
          sessionId: result.sessionId,
          sessionFile: result.sessionFile,
          error: result.error,
        });
      })
      .catch((err) => {
        unsub();
        stream.push({
          type: "turn_failed",
          session_id: this.deps.getSessionManager().getSessionId(),
          turn_id: "",
          error: err instanceof Error ? err.message : String(err),
          timestamp: Date.now(),
        });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      });

    return stream;
  }

  /**
   * Map old orchestrator wire event to unified HostEvent and publish to EventBus.
   */
  private publishToEventBus(
    event: OrchWireEvent,
    sessionId: string,
    _rootTaskId: string,
    turnId: string,
  ): void {
    const ts = Date.now();
    switch (event.type) {
      case "message_start":
        this.deps.eventBus.publish({
          type: "message_start",
          task_id: event.taskId,
          agent_id: event.agentId,
          message_id: event.message.id,
          role: event.message.role as "assistant" | "user" | "tool_result",
        });
        break;
      case "message_end":
        if (event.message.role === "assistant") {
          const text = event.message.content
            .filter((b): b is { type: "text"; text: string } => b.type === "text")
            .map((b) => b.text)
            .join("");
          const toolCalls = event.message.content
            .filter(
              (b): b is { type: "toolCall"; id: string; name: string; arguments: unknown } =>
                b.type === "toolCall",
            )
            .map((tc) => ({ id: tc.id, name: tc.name, args: tc.arguments }));
          this.deps.eventBus.publish({
            type: "assistant_message_completed",
            session_id: sessionId,
            message_id: event.message.id,
            task_id: event.taskId,
            agent_id: event.agentId,
            text,
            tool_calls: toolCalls,
            model: event.message.model ?? "unknown",
            provider: event.message.provider ?? "unknown",
            usage: event.message.usage,
            timestamp: ts,
          });
        }
        this.deps.eventBus.publish({
          type: "message_end",
          task_id: event.taskId,
          agent_id: event.agentId,
          message_id: event.message.id,
          stop_reason:
            "stopReason" in event.message
              ? (event.message as { stopReason?: string }).stopReason
              : undefined,
        });
        break;
      case "token":
        this.deps.eventBus.publish({
          type: "text_delta",
          task_id: event.taskId,
          agent_id: event.agentId,
          message_id: "",
          delta: event.text,
        });
        break;
      case "thinking":
        this.deps.eventBus.publish({
          type: "thinking_delta",
          task_id: event.taskId,
          agent_id: event.agentId,
          message_id: "",
          delta: event.text,
        });
        break;
      case "tool_start":
        this.deps.eventBus.publish({
          type: "tool_start",
          task_id: event.taskId,
          agent_id: event.agentId,
          tool_call_id: event.id,
          tool_name: event.name,
          args: event.args,
        });
        break;
      case "tool_end":
        this.deps.eventBus.publish({
          type: "tool_end",
          task_id: event.taskId,
          agent_id: event.agentId,
          tool_call_id: event.id,
          tool_name: event.name,
          result: event.result,
          is_error: event.isError,
        });
        break;
      case "approval_needed":
        this.deps.eventBus.publish({
          type: "approval_requested",
          task_id: event.taskId,
          agent_id: event.agentId,
          approval_id: event.approvalId,
          tool_name: event.toolName,
          tool_args: event.toolArgs,
        });
        break;
      case "approval_resolved":
        this.deps.eventBus.publish({
          type: "approval_resolved",
          task_id: event.taskId,
          agent_id: event.agentId,
          approval_id: event.approvalId,
          decision: event.decision as "accept" | "decline" | "accept_session" | "accept_workspace",
        });
        break;
      case "task_completed":
        this.deps.eventBus.publish({
          type: "task_completed",
          session_id: sessionId,
          task_id: event.taskId,
          agent_id: event.agentId,
          total_steps: 0,
          summary: event.result.summary,
          final_status: "completed",
          timestamp: ts,
        });
        break;
      case "task_failed":
        this.deps.eventBus.publish({
          type: "task_failed",
          session_id: sessionId,
          task_id: event.taskId,
          agent_id: event.agentId,
          error: event.error,
          timestamp: ts,
        });
        break;
      case "task_created":
        this.deps.eventBus.publish({
          type: "task_created",
          session_id: sessionId,
          task_id: event.task.id,
          agent_id: event.task.targetAgentId,
          parent_task_id: event.task.parentTaskId ?? null,
          source_agent_id: event.task.source.type === "agent" ? event.task.source.agentId : null,
          prompt: event.task.prompt,
          turn_id: turnId,
          timestamp: ts,
        });
        break;
      case "task_started":
        this.deps.eventBus.publish({
          type: "task_started",
          session_id: sessionId,
          task_id: event.taskId,
          agent_id: event.agentId,
          timestamp: ts,
        });
        break;
      case "task_transcript_committed":
        this.deps.eventBus.publish({
          type: "task_transcript_committed",
          session_id: sessionId,
          task_id: event.taskId,
          agent_id: event.agentId,
          parent_task_id: event.taskId,
          messages: event.messages,
          summary: event.summary,
          final_status: event.finalStatus,
          timestamp: ts,
        });
        break;
    }
  }

  private async runCore(
    prompt: string,
    signal?: AbortSignal,
    onStream?: (event: OrchWireEvent) => void,
    agentId = "main",
  ): Promise<HostRunResult> {
    const orch = this.deps.getOrchestrator();
    if (!orch) throw new Error("Orchestrator is not available");

    const config = this.deps.getConfig();
    const prepareSpan = startDebugSpan("host.run.prepare", { agentId });
    let prepared: Awaited<ReturnType<typeof prepareOrchestratorRun>>;
    try {
      prepared = await prepareOrchestratorRun({
        orch,
        agentId,
        agentName: agentId !== "main" ? this.deps.agentNameAssigner.next() : undefined,
        systemPrompt: this.deps.getSystemPrompt(),
        activeToolNames: this.deps.getActiveToolNames(),
        mcpServers: this.deps.getMcpServers(),
        mcpManager: this.deps.getMcpManager(),
      });
      prepareSpan.end({ outcome: "completed" });
    } catch (error) {
      prepareSpan.end({ outcome: "error" });
      throw error;
    }
    this.deps.setMcpManager(prepared.mcpManager);

    orch.unregisterAgent(agentId);
    orch.registerAgent(prepared.agentSpec);

    orch.setModelConfig({
      model: config.model,
      provider: config.provider,
      settings: config.settings,
    });

    this.deps.state.getAgentQueue(agentId);

    const sessionId = this.deps.getSessionManager().getSessionId();
    const turnId = `turn-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const rootTaskId = `task-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    // Publish unified turn/task lifecycle events
    this.deps.eventBus.publish({
      type: "turn_started",
      session_id: sessionId,
      turn_id: turnId,
      root_task_id: rootTaskId,
      timestamp: Date.now(),
    });

    this.deps.eventBus.publish({
      type: "task_created",
      session_id: sessionId,
      task_id: rootTaskId,
      agent_id: agentId,
      parent_task_id: null,
      source_agent_id: null,
      prompt,
      turn_id: turnId,
      timestamp: Date.now(),
    });

    this.deps.eventBus.publish({
      type: "task_started",
      session_id: sessionId,
      task_id: rootTaskId,
      agent_id: agentId,
      timestamp: Date.now(),
    });

    // Publish user message
    const userMessageId = `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this.deps.eventBus.publish({
      type: "user_message_submitted",
      session_id: sessionId,
      message_id: userMessageId,
      task_id: rootTaskId,
      text: prompt,
      timestamp: Date.now(),
    });

    const unsub = orch.subscribe((event) => {
      this.deps.persistence.enqueueEvent(event);
      if ("agentId" in event && event.agentId !== agentId) return;
      if (event.type === "task_created" && event.task.targetAgentId !== agentId) return;
      onStream?.(event);

      // Also publish unified events to EventBus
      this.publishToEventBus(event, sessionId, rootTaskId, turnId);
    });

    const history = await this.deps.persistence.loadAgentHistory(agentId);

    try {
      const runSpan = startDebugSpan("host.orchestrator.run", {
        agentId,
        signalAborted: signal?.aborted ?? false,
      });
      let result: Awaited<ReturnType<typeof orch.run>>;
      try {
        result = await orch.run(prompt, { targetAgentId: agentId, signal, history });
        runSpan.end({
          outcome: signal?.aborted ? "aborted" : result.status === "error" ? "error" : "completed",
          signalAborted: signal?.aborted ?? false,
          status: result.status,
        });
      } catch (error) {
        runSpan.end({
          outcome: signal?.aborted ? "aborted" : "error",
          signalAborted: signal?.aborted ?? false,
        });
        throw error;
      }
      const messages: Message[] = result.messages;

      const persistSpan = startDebugSpan("persistence.flush", { agentId });
      try {
        await this.deps.persistence.saveAgentMessages(agentId, messages);
        await this.deps.persistence.flush();
        persistSpan.end({ outcome: "completed" });
      } catch (error) {
        persistSpan.end({ outcome: "error" });
        throw error;
      }

      // Publish unified turn completed event
      this.deps.eventBus.publish({
        type: "turn_completed",
        session_id: sessionId,
        turn_id: turnId,
        total_tasks: 1,
        timestamp: Date.now(),
      });

      const sessionManager = this.deps.getSessionManager();
      return {
        messages,
        totalSteps: result.totalSteps,
        status: result.status,
        sessionId: sessionManager.getSessionId(),
        sessionFile: sessionManager.getSessionFile(),
        error: result.error,
      };
    } finally {
      unsub();
    }
  }
}
