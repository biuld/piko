import type {
  Model,
  ModelProviderConfig,
  ToolApprovalDecision,
  ToolApprovalRequest,
} from "../shared/index.js";
import { createDefaultSettings, createHostConfig } from "../shared/index.js";
import type { RunTuiOptions } from "./types.js";

export interface MakeHostOptionsExtras {
  approvalHandler?: (request: ToolApprovalRequest) => Promise<ToolApprovalDecision>;
}

export function makeHostOptions(
  model: Model<string>,
  providerConfig: ModelProviderConfig,
  sessionOptions: { session?: string },
  settingsManager: import("../shared/index.js").SettingsManager,
  tuiOptions?: RunTuiOptions,
  extras?: MakeHostOptionsExtras,
): Record<string, unknown> {
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
