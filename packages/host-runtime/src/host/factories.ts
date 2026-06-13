import { createNativeEngine, NativeToolProvider } from "piko-engine-native";
import { Orchestrator } from "piko-orchestrator";
import type { StatelessEngine, ToolDef } from "piko-protocol";
import { HostToolProvider } from "../host-provider.js";
import type { HostConfig } from "../models/index.js";
import { PikoSessionRuntime, type SessionManager } from "../session/index.js";
import { PikoHost } from "./index.js";
import type { HostToolHandlers, PikoHostCreateOptions, ToolApprovalHandler } from "./types.js";

function createHostToolProvider(opts: {
  approvalHandler?: ToolApprovalHandler;
  hostToolHandlers?: HostToolHandlers;
}): HostToolProvider {
  const provider = new HostToolProvider();

  for (const [toolName, handler] of Object.entries(opts.hostToolHandlers ?? {})) {
    if (handler) provider.setHandler(toolName, handler);
  }

  if (opts.approvalHandler && !opts.hostToolHandlers?.request_approval) {
    provider.setHandler("request_approval", async (args, context, call) => {
      const action = typeof args.action === "string" ? args.action : "request_approval";
      const decision = await opts.approvalHandler!({
        callId: call.id,
        agentId: context.agentId,
        taskId: context.taskId,
        toolName: action,
        toolArgs: args,
      });

      return {
        approved: decision === "accept",
        decision,
      };
    });
  }

  return provider;
}

export async function createPikoHost(options: PikoHostCreateOptions): Promise<PikoHost> {
  const sessionRuntime = await PikoSessionRuntime.create(options.session);

  const customToolDefs: ToolDef[] | undefined = options.customTools?.map((t) => ({
    name: t.name,
    description: t.description,
    inputSchema: t.inputSchema as ToolDef["inputSchema"],
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

  const orchestrator = options.orchestrator ?? new Orchestrator(engine);

  // Register tool providers
  orchestrator.registerProvider(new NativeToolProvider(customToolRegistry ?? {}));
  if (options.approvalHandler) {
    orchestrator.setApprovalGateway({
      requestToolApproval: options.approvalHandler,
    });
  }
  orchestrator.registerProvider(
    createHostToolProvider({
      approvalHandler: options.approvalHandler,
      hostToolHandlers: options.hostToolHandlers,
    }),
  );

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
    hostToolHandlers?: PikoHostCreateOptions["hostToolHandlers"];
    systemPrompt?: string;
    settingsManager?: PikoHostCreateOptions["settingsManager"];
  } = {},
): PikoHost {
  const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
  const orchestrator = new Orchestrator(engine);
  orchestrator.registerProvider(new NativeToolProvider({}));
  if (options.approvalHandler) {
    orchestrator.setApprovalGateway({
      requestToolApproval: options.approvalHandler,
    });
  }
  orchestrator.registerProvider(
    createHostToolProvider({
      approvalHandler: options.approvalHandler,
      hostToolHandlers: options.hostToolHandlers,
    }),
  );
  return new PikoHost(engine, config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    settingsManager: options.settingsManager,
    orchestrator,
  });
}
