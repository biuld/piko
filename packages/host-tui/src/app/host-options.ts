import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { createDefaultSettings, createHostConfig, type PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "./types.js";

export function makeHostOptions(
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  sessionOptions: { session?: string },
  settingsManager?: import("piko-host-runtime").SettingsManager,
  tuiOptions?: RunTuiOptions,
): Parameters<typeof PikoHost.create>[0] {
  return {
    config: createHostConfig(
      model,
      providerConfig,
      createDefaultSettings({
        maxSteps: 10,
        parallelTools: false,
        allowToolCalls: !tuiOptions?.noTools,
        allowApprovals: true,
      }),
    ),
    session: sessionOptions,
    settingsManager,
    systemPrompt: tuiOptions?.systemPrompt,
    appendSystemPrompt: tuiOptions?.appendSystemPrompt,
    skipContextFiles: tuiOptions?.noContextFiles,
  };
}
