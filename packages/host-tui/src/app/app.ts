import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { ResolvedModel } from "piko-host-runtime";
import { BaseApp } from "./base.js";
import { doUpdateHeader, doUpdateFooter } from "./header-footer.js";
import { doSyncTranscript, doResume, doNewSession, doClone, doFork } from "./session.js";
import { doGetModelList, doGetModelIds, doResolveModel, doApplyModelChange, doCycleModel } from "./model.js";
import { doSubmit, doSubmitStream } from "./submit.js";
import { buildCommandContext } from "./commands-ctx.js";
import { initApp } from "./init.js";
import type { RunTuiOptions } from "./types.js";

export class App extends BaseApp {
  // ---- Header / Footer ----
  updateHeader(): void { doUpdateHeader(this); }
  updateFooter(): void { doUpdateFooter(this); }

  // ---- Session ----
  syncTranscript(msg?: string): Promise<void> { return doSyncTranscript(this, msg); }
  resume(): Promise<void> { return doResume(this); }
  newSession(): Promise<void> { return doNewSession(this); }
  clone(): Promise<void> { return doClone(this); }
  fork(entryId: string): Promise<void> { return doFork(this, entryId); }

  // ---- Model ----
  getModelList(): ReturnType<typeof doGetModelList> { return doGetModelList(this); }
  getModelIds(): string[] { return doGetModelIds(this); }
  resolveModel(id: string, prov: string): ResolvedModel | null { return doResolveModel(this, id, prov); }
  applyModelChange(found: ResolvedModel): void { doApplyModelChange(this, found); }
  async cycleModel(forward: boolean): Promise<void> { return doCycleModel(this, forward); }

  // ---- Submit ----
  submit(text: string): void { doSubmit(this, text); }
  submitStream(factory: (sig: AbortSignal) => ReturnType<BaseApp["host"]["streamPrompt"]>, label: string): void { doSubmitStream(this, factory, label); }

  // ---- Commands Ctx ----
  buildCommandContext(): ReturnType<typeof buildCommandContext> { return buildCommandContext(this); }

  // ---- Init ----
  init(options: RunTuiOptions): Promise<void> { return initApp(this, options); }
}
