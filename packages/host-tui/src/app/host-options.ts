import type { Model } from "@earendil-works/pi-ai";
import { createDefaultSettings, createHostConfig, type PikoHost } from "piko-host-runtime";
import type { ModelProviderConfig } from "piko-orchestrator";
import type { RunTuiOptions } from "./types.js";

export function makeHostOptions(
  model: Model<string>,
  providerConfig: ModelProviderConfig,
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
        allowToolCalls: !tuiOptions?.noTools,
      }),
    ),
    session: sessionOptions,
    settingsManager,
    systemPrompt: tuiOptions?.systemPrompt,
    appendSystemPrompt: tuiOptions?.appendSystemPrompt,
    skipContextFiles: tuiOptions?.noContextFiles,
  };
}
