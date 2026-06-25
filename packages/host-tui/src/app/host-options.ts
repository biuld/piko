import { createDefaultSettings, createHostConfig, type PikoHost } from "piko-host-runtime";
import type {
  Model,
  ModelProviderConfig,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "piko-orch-protocol";
import type { RunTuiOptions } from "./types.js";

export interface MakeHostOptionsExtras {
  approvalHandler?: (request: ToolApprovalRequest) => Promise<ToolApprovalDecision>;
}

export function makeHostOptions(
  model: Model<string>,
  providerConfig: ModelProviderConfig,
  sessionOptions: { session?: string },
  settingsManager: import("piko-host-runtime").SettingsManager,
  tuiOptions?: RunTuiOptions,
  extras?: MakeHostOptionsExtras,
): Parameters<typeof PikoHost.create>[0] {
  return {
    config: createHostConfig(
      model,
      providerConfig,
      createDefaultSettings({
        allowToolCalls: !tuiOptions?.noTools,
      }),
    ),
    session: sessionOptions,
    settingsManager,
    systemPrompt: tuiOptions?.systemPrompt,
    appendSystemPrompt: tuiOptions?.appendSystemPrompt,
    skipContextFiles: tuiOptions?.noContextFiles,
    modelRegistry: tuiOptions?.modelRegistry,
    approvalHandler: extras?.approvalHandler,
  };
}
