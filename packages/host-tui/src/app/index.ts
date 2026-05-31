import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { PikoHost } from "piko-host-runtime";
import { InteractiveApprovalHandler } from "../approval-handler.js";
import { BaseApp } from "./base.js";
import { HeaderFooterMixin } from "./header-footer.js";
import { makeHostOptions } from "./host-options.js";
import { InitMixin } from "./init.js";
import { ModelMixin } from "./model.js";
import { SessionMixin } from "./session.js";
import { SubmitMixin } from "./submit.js";
import { CommandsCtxMixin } from "./commands-ctx.js";
import type { RunTuiOptions } from "./types.js";

export type { RunTuiOptions } from "./types.js";

// Compose all mixins into the final App class
const _App = InitMixin(CommandsCtxMixin(SubmitMixin(ModelMixin(SessionMixin(HeaderFooterMixin(BaseApp))))));
export class App extends _App {}

export async function runTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const host = await PikoHost.create({
    ...makeHostOptions(initialModel, initialProviderConfig, { session: options.session }, options.settingsManager, options),
    approvalHandler: new InteractiveApprovalHandler(null!),
  });

  const app = new App(initialModel, initialProviderConfig, options, host);
  (host as any).approvalHandler = new InteractiveApprovalHandler(app.tui);
  await app.init(options);
}
