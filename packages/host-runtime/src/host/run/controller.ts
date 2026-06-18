import { EventStream, type ModelStepEvent, type Orchestrator } from "piko-orchestrator";
import type { HostEvent, HostRuntimeEvent, Message } from "piko-orchestrator-protocol";

import type { HostConfig } from "../../models/index.js";
import type { SessionManager } from "../../session/index.js";
import type { McpServerManager } from "../../tools/mcp-provider.js";
import type { HostPersistence } from "../persistence/index.js";
import type { HostRunResult, StreamPromptOptions, StreamPromptResult } from "../shared/index.js";
import type { HostState } from "../state/index.js";
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
          error: err instanceof Error ? err.message : String(err),
          aborted: signal?.aborted ?? false,
        });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      });

    return stream;
  }

  private projectHostEvent(event: HostEvent, _agentId: string): HostRuntimeEvent | null {
    switch (event.type) {
      case "task_started":
        return {
          type: "agent_start",
          runId: event.taskId,
          agentId: event.agentId,
        };
      case "message_start":
        return {
          type: "message_start",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
        };
      case "message_update":
        return {
          type: "message_update",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
          assistantEvent: event.assistantEvent,
        };
      case "message_end":
        return {
          type: "message_end",
          runId: event.taskId,
          agentId: event.agentId,
          message: event.message,
        };
      case "tool_start":
        return {
          type: "tool_execution_start",
          runId: event.taskId,
          agentId: event.agentId,
          toolCallId: event.id,
          toolName: event.name,
          args: event.args,
        };
      case "tool_end":
        return {
          type: "tool_execution_end",
          runId: event.taskId,
          agentId: event.agentId,
          toolCallId: event.id,
          toolName: event.name,
          result: event.result,
          isError: event.isError,
        };
      case "task_completed":
        return {
          type: "agent_end",
          runId: event.taskId,
          agentId: event.agentId,
          status: "completed",
        };
      case "task_failed":
        return {
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
    const prepared = await prepareOrchestratorRun({
      orch,
      config,
      agentId,
      systemPrompt: this.deps.getSystemPrompt(),
      activeToolNames: this.deps.getActiveToolNames(),
      mcpServers: this.deps.getMcpServers(),
      mcpManager: this.deps.getMcpManager(),
    });
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
      if (event.type === "task_completed") {
        const msgs = orch.snapshot().agents[agentId]?.transcript ?? [];
        this.deps.persistence.saveAgentMessages(agentId, msgs).catch(() => {});
      }
    });

    const history = await this.deps.persistence.loadAgentHistory(agentId);

    try {
      const result = await orch.run(prompt, { targetAgentId: agentId, signal, history });
      const messages: Message[] = result.messages;

      await this.deps.persistence.saveAgentMessages(agentId, messages);
      await this.deps.persistence.flush();

      const sessionManager = this.deps.getSessionManager();
      return {
        messages,
        totalSteps: result.totalSteps,
        status:
          result.status === "max_steps"
            ? "max_steps"
            : result.status === "error"
              ? "error"
              : "completed",
        sessionId: sessionManager.getSessionId(),
        sessionFile: sessionManager.getSessionFile(),
      };
    } finally {
      unsub();
    }
  }
}
