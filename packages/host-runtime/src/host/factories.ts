import type { ModelStepExecutor } from "piko-orchestrator";
import { createModelCaller, Orchestrator } from "piko-orchestrator";
import type { HostConfig } from "../models/index.js";
import { loadPromptTemplates } from "../prompts/index.js";
import type { ExecutionEnv } from "../session/exec-env.js";
import { PikoSessionRuntime, SandboxExecutionEnv, type SessionManager } from "../session/index.js";
import { SettingsManager } from "../settings/index.js";
import { loadSkills } from "../skills/index.js";
import { HostToolProvider } from "../tools/host-provider.js";
import { WorkspaceToolProvider } from "../tools/workspace-provider.js";
import { PikoHost } from "./index.js";
import { buildEnhancedSystemPromptEngines } from "./resources/index.js";
import { builtinToolSet } from "./run/toolsets.js";
import type {
  HostToolCallbacks,
  PikoHostCreateOptions,
  ToolApprovalHandler,
} from "./shared/index.js";

function buildHostCallbacks(opts: {
  approvalHandler?: ToolApprovalHandler;
  hostToolCallbacks?: HostToolCallbacks;
}): HostToolCallbacks {
  const callbacks: HostToolCallbacks = { ...opts.hostToolCallbacks };

  // Wire approval handler into requestApproval if not explicitly provided
  if (opts.approvalHandler && !callbacks.requestApproval) {
    callbacks.requestApproval = async (action, _details) => {
      const decision = await opts.approvalHandler!({
        toolEntityId: "",
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

function configuredExecutionEnv(
  sessionManager: SessionManager,
  settingsManager: SettingsManager,
): ExecutionEnv {
  const sandbox = settingsManager.getSandboxSettings();
  if (sandbox.enabled && !(sessionManager.getExecutionEnv() instanceof SandboxExecutionEnv)) {
    sessionManager.setExecutionEnv(
      new SandboxExecutionEnv({ cwd: sessionManager.getCwd(), ...sandbox }),
    );
  }
  return sessionManager.getExecutionEnv();
}

export async function createPikoHost(options: PikoHostCreateOptions): Promise<PikoHost> {
  const sessionRuntime = await PikoSessionRuntime.create(options.session);
  const settingsManager = options.settingsManager ?? SettingsManager.inMemory();

  const engine: ModelStepExecutor = options.engine ?? createModelCaller();
  const config = options.config;

  const orchestrator = options.orchestrator ?? new Orchestrator(engine, config);
  const cwd = sessionRuntime.getCwd();
  const promptTemplates = options.promptTemplates ?? (await loadPromptTemplates({ cwd }));
  const skills = (await loadSkills({ cwd })).skills;
  const systemPrompt =
    options.systemPrompt ??
    (await buildEnhancedSystemPromptEngines(
      engine.capabilities.tools,
      cwd,
      options.appendSystemPrompt,
      options.promptGuidelines,
      promptTemplates,
      options.skipContextFiles,
    ));

  orchestrator.registerProvider(
    new WorkspaceToolProvider(() =>
      configuredExecutionEnv(sessionRuntime.getSessionManager(), settingsManager),
    ),
  );
  orchestrator.registerToolSet(builtinToolSet);
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

  const host = new PikoHost(config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt,
    appendSystemPrompt: options.appendSystemPrompt,
    promptGuidelines: options.promptGuidelines,
    promptTemplates,
    skills,
    settingsManager,
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
  const settingsManager = options.settingsManager ?? SettingsManager.inMemory();
  const orchestrator = new Orchestrator(engine, config);

  orchestrator.registerProvider(
    new WorkspaceToolProvider(() => configuredExecutionEnv(sessionManager, settingsManager)),
  );
  orchestrator.registerToolSet(builtinToolSet);
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

  return new PikoHost(config, sessionRuntime, {
    approvalHandler: options.approvalHandler,
    systemPrompt: options.systemPrompt,
    skills: [],
    settingsManager,
    orchestrator,
  });
}
