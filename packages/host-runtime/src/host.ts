import type {
  Message,
  EngineTool,
  StatelessEngine,
  EngineEvent,
  EngineRunSettings,
  EngineStepResult,
  EventStream,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { HostConfig } from "./model-config.js";
import type { ApprovalHandler } from "./approval-controller.js";
import { createSession, addUserMessage } from "./session-store.js";
import { runScheduler } from "./scheduler.js";
import { SessionManager } from "./session-manager.js";

export interface HostRunOptions {
  engine: StatelessEngine;
  config: HostConfig;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  systemPrompt?: string;
  sessionManager?: SessionManager;
  cwd?: string;
}

export interface StreamPromptOptions {
  settingsOverride?: Partial<EngineRunSettings>;
}

export interface StreamPromptResult {
  messages: Message[];
  appendedMessages: Message[];
  status: EngineStepResult["status"];
  sessionId: string;
  sessionFile?: string;
}

export class PikoHost {
  private engine: StatelessEngine;
  private config: HostConfig;
  private tools?: EngineTool[];
  private approvalHandler?: ApprovalHandler;
  private systemPrompt: string;
  private sessionManager?: SessionManager;
  private cwd: string;

  constructor(options: HostRunOptions) {
    this.engine = options.engine;
    this.config = options.config;
    this.tools = options.tools;
    this.approvalHandler = options.approvalHandler;
    this.systemPrompt = options.systemPrompt ?? "You are a helpful assistant.";
    this.sessionManager = options.sessionManager;
    this.cwd = options.cwd ?? process.cwd();
  }

  private async getOrCreateSessionManager(): Promise<SessionManager> {
    if (this.sessionManager) return this.sessionManager;
    this.sessionManager = await SessionManager.create(this.cwd);
    return this.sessionManager;
  }

  private async loadSessionState(): Promise<{
    sessionManager: SessionManager;
    session: ReturnType<typeof createSession>;
  }> {
    const sessionManager = await this.getOrCreateSessionManager();
    const existingMessages = await sessionManager.loadMessages();
    const session = createSession({
      sessionId: sessionManager.getSessionId(),
      messages: existingMessages,
      systemPrompt: this.systemPrompt,
    });
    return { sessionManager, session };
  }

  streamPrompt(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();

    void this.loadSessionState()
      .then(async ({ sessionManager, session }) => {
        const nextSession = addUserMessage(session, prompt);
        const input = {
          runId: `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          stepId: `step-0-${Date.now()}`,
          transcript: nextSession.messages,
          systemPrompt: nextSession.systemPrompt,
          model: this.config.model,
          provider: this.config.provider,
          tools: this.tools ?? [],
          settings: {
            ...this.config.settings,
            ...options.settingsOverride,
          },
          pendingApproval: nextSession.pendingApproval,
          engineState: nextSession.engineState,
        };

        const engineStream = this.engine.executeStep(input, signal);
        for await (const event of engineStream) {
          stream.push(event);
        }

        const result = await engineStream.result();
        const messages = [...nextSession.messages, ...result.appendedMessages];
        await sessionManager.saveMessages(this.config.model.id, messages);
        stream.end({
          messages,
          appendedMessages: result.appendedMessages,
          status: result.status,
          sessionId: sessionManager.getSessionId(),
          sessionFile: sessionManager.getSessionFile(),
        });
      })
      .catch((err) => {
        const message = err instanceof Error ? err.message : String(err);
        stream.push({ type: "error", message });
        stream.end({
          messages: [],
          appendedMessages: [],
          status: "error",
          sessionId: "",
        });
      });

    return stream;
  }

  async run(
    prompt: string,
    signal?: AbortSignal,
  ): Promise<{
    messages: Message[];
    totalSteps: number;
    status: "completed" | "aborted" | "error" | "max_steps";
    sessionId: string;
    sessionFile?: string;
  }> {
    const { sessionManager, session: loadedSession } = await this.loadSessionState();
    let session = loadedSession;
    session = addUserMessage(session, prompt);

    const result = await runScheduler({
      engine: this.engine,
      config: this.config,
      session,
      tools: this.tools,
      approvalHandler: this.approvalHandler,
      signal,
    });
    await sessionManager.saveMessages(this.config.model.id, result.session.messages);

    return {
      messages: result.session.messages,
      totalSteps: result.totalSteps,
      status: result.status,
      sessionId: sessionManager.getSessionId(),
      sessionFile: sessionManager.getSessionFile(),
    };
  }
}
