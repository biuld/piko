import {
  EventStream,
  type HostEvent,
  type HostRuntimeEvent,
  type Message,
  type Orchestrator,
  startDebugSpan,
} from "piko-orch-protocol";
import type { ModelStepEvent } from "../../models/executor.js";

import type { HostConfig } from "../../models/index.js";
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
    },
  ) {}

  async run(prompt: string, signal?: AbortSignal, agentId = "main"): Promise<HostRunResult> {
    return await this.runCore(prompt, signal, undefined, agentId);
  }

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
        });
      })
      .catch((err) => {
        stream.push({ type: "error", message: err instanceof Error ? err.message : String(err) });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      });

    return stream;
  }

  streamPromptLifecycle(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<HostRuntimeEvent, StreamPromptResult> {
    const stream = new EventStream<HostRuntimeEvent, StreamPromptResult>();
    const agentId = options.agentId ?? "main";

    void this.runCore(
      prompt,
      signal,
      (event) => {
        const projected = this.projectHostEvent(event, agentId);
        if (projected) {
          stream.push(projected);
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
        });
      })
      .catch((err) => {
        stream.push({
          type: "failure",
          runId: "",
          agentId,
          eventSeq: 0,
          turnIndex: 0,
          error: err instanceof Error ? err.message : String(err),
          aborted: signal?.aborted ?? false,
        });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      });

    return stream;
  }

  private projectHostEvent(event: HostEvent, _agentId: string): HostRuntimeEvent | null {
    const ev = event as unknown as Record<string, unknown>;
    const order =
      "eventSeq" in ev
        ? {
            eventSeq: ev.eventSeq as number,
            turnIndex: ev.turnIndex as number,
            messageIndex: ev.messageIndex as number | undefined,
          }
        : { eventSeq: 0, turnIndex: 0, messageIndex: undefined };

    switch (event.type) {
      case "task_started":
        return {
          ...order,
          type: "agent_start",
          runId: event.taskId,
          agentId: event.agentId,
        };
      case "message_start":
        return {
          ...order,
          type: "message_start",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
        };
      case "message_update":
        return {
          ...order,
          type: "message_update",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
          assistantEvent: event.assistantEvent,
        };
      case "message_end":
        return {
          ...order,
          type: "message_end",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
        };
      case "tool_start":
        return {
          ...order,
          parentMessageId: (ev.parentMessageId as string) ?? "",
          contentIndex: (ev.contentIndex as number) ?? 0,
          toolCallIndex: (ev.toolCallIndex as number) ?? 0,
          type: "tool_execution_start",
          toolEntityId: event.entityId,
          runId: event.taskId,
          agentId: event.agentId,
          toolCallId: event.id,
          toolName: event.name,
          args: event.args,
        };
      case "tool_end":
        return {
          ...order,
          parentMessageId: (ev.parentMessageId as string) ?? "",
          contentIndex: (ev.contentIndex as number) ?? 0,
          toolCallIndex: (ev.toolCallIndex as number) ?? 0,
          type: "tool_execution_end",
          toolEntityId: event.entityId,
          runId: event.taskId,
          agentId: event.agentId,
          toolCallId: event.id,
          toolName: event.name,
          result: event.result,
          isError: event.isError,
        };
      case "task_completed":
        return {
          ...order,
          type: "agent_end",
          runId: event.taskId,
          agentId: event.agentId,
          status: "completed",
        };
      case "task_failed":
        return {
          ...order,
          type: "failure",
          runId: event.taskId,
          agentId: event.agentId,
          error: event.error,
          aborted: event.error.includes("aborted") || event.error.includes("cancel"),
        };
      default:
        return null;
    }
  }

  private async runCore(
    prompt: string,
    signal?: AbortSignal,
    onStream?: (event: HostEvent) => void,
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

    const unsub = orch.subscribe((event) => {
      this.deps.persistence.enqueueEvent(event);
      if ("agentId" in event && event.agentId !== agentId) return;
      if (event.type === "task_created" && event.task.targetAgentId !== agentId) return;
      onStream?.(event);
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

      const sessionManager = this.deps.getSessionManager();
      return {
        messages,
        totalSteps: result.totalSteps,
        status: result.status,
        sessionId: sessionManager.getSessionId(),
        sessionFile: sessionManager.getSessionFile(),
      };
    } finally {
      unsub();
    }
  }
}
