import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { PikoHost } from "piko-host-runtime";
import { InteractiveApprovalHandler } from "../approval-handler.js";
import { App } from "./app.js";
import { makeHostOptions } from "./host-options.js";
import type { RunTuiOptions } from "./types.js";

export type { RunTuiOptions } from "./types.js";

export async function runTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const host = await PikoHost.create({
    ...makeHostOptions(initialModel, initialProviderConfig, { session: options.session }, options.settingsManager, options),
    approvalHandler: new InteractiveApprovalHandler(null!), // replaced after TUI creation
    customTools: undefined, // extensions loaded later in App.init
  });

  const app = new App(initialModel, initialProviderConfig, options, host);

  // Patch approval handler now that tui exists
  (host as any).approvalHandler = new InteractiveApprovalHandler(app.tui);

  await app.init(options);
}
