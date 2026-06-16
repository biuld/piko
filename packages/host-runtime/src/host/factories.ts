import type { ModelStepExecutor } from "piko-orchestrator";
import { createModelCaller, Orchestrator } from "piko-orchestrator";
import type { ToolDef } from "piko-orchestrator-protocol";
import type { HostConfig } from "../models/index.js";
import { PikoSessionRuntime, type SessionManager } from "../session/index.js";
import { HostToolProvider } from "../tools/host-provider.js";
import { WorkspaceToolProvider } from "../tools/workspace-provider.js";
import { PikoHost } from "./index.js";
import type { HostToolCallbacks, PikoHostCreateOptions, ToolApprovalHandler } from "./types.js";

function buildHostCallbacks(opts: {
  approvalHandler?: ToolApprovalHandler;
  hostToolCallbacks?: HostToolCallbacks;
}): HostToolCallbacks {
  const callbacks: HostToolCallbacks = { ...opts.hostToolCallbacks };

  // Wire approval handler into requestApproval if not explicitly provided
  if (opts.approvalHandler && !callbacks.requestApproval) {
    callbacks.requestApproval = async (action, _details) => {
      const decision = await opts.approvalHandler!({
        callId: "",
        agentId: "",
        taskId: "",
        toolName: action,
        toolArgs: { action },
      });
      return { approved: decision === "accept", decision };
    };
  }

  return callbacks;
}

export async function createPikoHost(options: PikoHostCreateOptions): Promise<PikoHost> {
  const sessionRuntime = await PikoSessionRuntime.create(options.session);
  const execEnv = sessionRuntime.getSessionManager().getExecutionEnv();

  const customToolDefs: ToolDef[] | undefined = options.customTools?.map((t) => ({
    name: t.name,
    description: t.description,
    inputSchema: t.inputSchema as ToolDef["inputSchema"],
    executor: { kind: "native" as const, target: t.name },
  }));

  const engine: ModelStepExecutor =
    options.engine ??
    createModelCaller({
      toolDefinitions: customToolDefs,
    });
  const config =
    customToolDefs?.length && !options.config.tools?.some((tool) => customToolDefs.includes(tool))
      ? { ...options.config, tools: [...(options.config.tools ?? []), ...customToolDefs] }
      : options.config;

  const orchestrator = options.orchestrator ?? new Orchestrator(engine);

  // Register tool providers
  orchestrator.registerProvider(
    new WorkspaceToolProvider(execEnv, { customTools: options.customTools }),
  );
  if (options.approvalHandler) {
    orchestrator.setApprovalGateway({
      requestToolApproval: options.approvalHandler,
    });
  }
  orchestrator.registerProvider(
    new HostToolProvider(
      buildHostCallbacks({
        approvalHandler: options.approvalHandler,
        hostToolCallbacks: options.hostToolCallbacks,
      }),
    ),
  );

  const host = new PikoHost(engine, config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    appendSystemPrompt: options.appendSystemPrompt,
    promptGuidelines: options.promptGuidelines,
    promptTemplates: options.promptTemplates,
    settingsManager: options.settingsManager,
    skipContextFiles: options.skipContextFiles,
    orchestrator,
    modelRegistry: options.modelRegistry,
  });

  return host;
}

export function createPikoHostFromSessionManager(
  engine: ModelStepExecutor,
  config: HostConfig,
  sessionManager: SessionManager,
  options: {
    approvalHandler?: PikoHostCreateOptions["approvalHandler"];
    hostToolCallbacks?: PikoHostCreateOptions["hostToolCallbacks"];
    systemPrompt?: string;
    settingsManager?: PikoHostCreateOptions["settingsManager"];
  } = {},
): PikoHost {
  const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
  const execEnv = sessionManager.getExecutionEnv();
  const orchestrator = new Orchestrator(engine);

  orchestrator.registerProvider(new WorkspaceToolProvider(execEnv, { extraDefs: config.tools }));
  if (options.approvalHandler) {
    orchestrator.setApprovalGateway({
      requestToolApproval: options.approvalHandler,
    });
  }
  orchestrator.registerProvider(
    new HostToolProvider(
      buildHostCallbacks({
        approvalHandler: options.approvalHandler,
        hostToolCallbacks: options.hostToolCallbacks,
      }),
    ),
  );

  return new PikoHost(engine, config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    settingsManager: options.settingsManager,
    orchestrator,
  });
}
