import type { Message } from "@earendil-works/pi-ai";
import type { StatelessEngine, EngineEvent, EngineTool } from "piko-engine-protocol";
import type { HostConfig } from "./model-config.js";
import type { ApprovalHandler } from "./approval-controller.js";
import { createSession, addUserMessage } from "./session-store.js";
import { runScheduler } from "./scheduler.js";

export interface HostRunOptions {
  engine: StatelessEngine;
  config: HostConfig;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  systemPrompt?: string;
}

export class PikoHost {
  private engine: StatelessEngine;
  private config: HostConfig;
  private tools?: EngineTool[];
  private approvalHandler?: ApprovalHandler;
  private systemPrompt: string;

  constructor(options: HostRunOptions) {
    this.engine = options.engine;
    this.config = options.config;
    this.tools = options.tools;
    this.approvalHandler = options.approvalHandler;
    this.systemPrompt = options.systemPrompt ?? "You are a helpful assistant.";
  }

  async run(
    prompt: string,
    signal?: AbortSignal,
  ): Promise<{
    messages: Message[];
    totalSteps: number;
    status: "completed" | "aborted" | "error" | "max_steps";
  }> {
    let session = createSession(this.systemPrompt);
    session = addUserMessage(session, prompt);

    const events: EngineEvent[] = [];

    const result = await runScheduler({
      engine: this.engine,
      config: this.config,
      session,
      tools: this.tools,
      approvalHandler: this.approvalHandler,
      signal,
      onEvent: (event) => {
        events.push(event);
      },
    });

    return {
      messages: result.session.messages,
      totalSteps: result.totalSteps,
      status: result.status,
    };
  }
}
