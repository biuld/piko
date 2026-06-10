import { AgentOrchestrator } from "piko-agent-orchestrator";
import { createNativeEngine } from "piko-engine-native";
import type { EngineTool, StatelessEngine } from "piko-engine-protocol";
import type { HostConfig } from "../models/index.js";
import { PikoSessionRuntime, type SessionManager } from "../session/index.js";
import { PikoHost } from "./index.js";
import type { PikoHostCreateOptions } from "./types.js";

export async function createPikoHost(options: PikoHostCreateOptions): Promise<PikoHost> {
  const sessionRuntime = await PikoSessionRuntime.create(options.session);

  const customToolDefs: EngineTool[] | undefined = options.customTools?.map((t) => ({
    name: t.name,
    description: t.description,
    inputSchema: t.inputSchema as EngineTool["inputSchema"],
    executor: { kind: "native" as const, target: t.name },
  }));
  const customToolRegistry:
    | Record<string, (args: Record<string, unknown>) => Promise<unknown>>
    | undefined = options.customTools?.reduce(
    (acc, t) => {
      acc[t.name] = (args: Record<string, unknown>) => Promise.resolve(t.executor(args));
      return acc;
    },
    {} as Record<string, (args: Record<string, unknown>) => Promise<unknown>>,
  );

  const engine =
    options.engine ??
    createNativeEngine({
      cwd: sessionRuntime.getCwd(),
      toolRegistry: customToolRegistry,
      toolDefinitions: customToolDefs,
    });

  const orchestrator = options.orchestrator ?? new AgentOrchestrator(engine);

  const host = new PikoHost(engine, options.config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    appendSystemPrompt: options.appendSystemPrompt,
    promptGuidelines: options.promptGuidelines,
    promptTemplates: options.promptTemplates,
    settingsManager: options.settingsManager,
    skipContextFiles: options.skipContextFiles,
    orchestrator,
  });

  return host;
}

export function createPikoHostFromSessionManager(
  engine: StatelessEngine,
  config: HostConfig,
  sessionManager: SessionManager,
  options: {
    approvalHandler?: PikoHostCreateOptions["approvalHandler"];
    systemPrompt?: string;
    settingsManager?: PikoHostCreateOptions["settingsManager"];
  } = {},
): PikoHost {
  const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
  const orchestrator = new AgentOrchestrator(engine);
  return new PikoHost(engine, config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    settingsManager: options.settingsManager,
    orchestrator,
  });
}
