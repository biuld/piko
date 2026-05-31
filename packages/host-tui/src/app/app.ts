import type { ResolvedModel } from "piko-host-runtime";
import { BaseApp } from "./base.js";
import { buildCommandContext } from "./commands-ctx.js";
import { doUpdateFooter, doUpdateHeader } from "./header-footer.js";
import { initApp } from "./init.js";
import {
  doApplyModelChange,
  doCycleModel,
  doGetModelIds,
  doGetModelList,
  doResolveModel,
} from "./model.js";
import { doClone, doFork, doNewSession, doResume, doSyncTranscript } from "./session.js";
import { doSubmit, doSubmitStream } from "./submit.js";
import type { RunTuiOptions } from "./types.js";

export class App extends BaseApp {
  // ---- Header / Footer ----
  updateHeader(): void {
    doUpdateHeader(this);
  }
  updateFooter(): void {
    doUpdateFooter(this);
  }

  // ---- Session ----
  syncTranscript(msg?: string): Promise<void> {
    return doSyncTranscript(this, msg);
  }
  resume(): Promise<void> {
    return doResume(this);
  }
  newSession(): Promise<void> {
    return doNewSession(this);
  }
  clone(): Promise<void> {
    return doClone(this);
  }
  fork(entryId: string): Promise<void> {
    return doFork(this, entryId);
  }

  // ---- Model ----
  getModelList(): ReturnType<typeof doGetModelList> {
    return doGetModelList(this);
  }
  getModelIds(): string[] {
    return doGetModelIds(this);
  }
  resolveModel(id: string, prov: string): ResolvedModel | null {
    return doResolveModel(this, id, prov);
  }
  applyModelChange(found: ResolvedModel): void {
    doApplyModelChange(this, found);
  }
  async cycleModel(forward: boolean): Promise<void> {
    return doCycleModel(this, forward);
  }

  // ---- Submit ----
  submit(text: string): void {
    doSubmit(this, text);
  }
  submitStream(
    factory: (sig: AbortSignal) => ReturnType<BaseApp["host"]["streamPrompt"]>,
    label: string,
    kind?: "skill" | "template",
  ): void {
    doSubmitStream(this, factory, label, kind);
  }

  // ---- Commands Ctx ----
  buildCommandContext(): ReturnType<typeof buildCommandContext> {
    return buildCommandContext(this);
  }

  // ---- Init ----
  init(options: RunTuiOptions): Promise<void> {
    return initApp(this, options);
  }
}
