import { findModel, listAvailableModels, type SettingsManager } from "piko-host-runtime";
import type { CommandContext } from "../commands/index.js";
import type { ResolvedModel } from "piko-host-runtime";
import {
  openForkSelector, openLoginDialog, openModelScopeSelector, openModelSelector,
  openResumeSelector, openSettingsSelector, openThinkingSelector, openTreeSelector,
} from "../overlays/index.js";
import { formatSessionTreeLines } from "../session-tree.js";
import { getThemeManager } from "../theme/index.js";
import { setTheme } from "../theme.js";
import type { BaseApp } from "./base.js";

export interface CommandsCtxDeps extends BaseApp {
  updateHeader(): void;
  updateFooter(): void;
  syncTranscript(msg?: string): Promise<void>;
  resume(): Promise<void>;
  newSession(): Promise<void>;
  clone(): Promise<void>;
  fork(entryId: string): Promise<void>;
  getModelList(): Array<{ model: any; providerConfig: any }>;
  applyModelChange(found: ResolvedModel): void;
  cycleModel(forward: boolean): Promise<void>;
  submit(text: string): void;
  submitStream(factory: (sig: AbortSignal) => any, label: string): void;
}

export function buildCommandContext(app: CommandsCtxDeps): CommandContext {
  const oc = {
    tui: app.tui, host: app.host,
    msg: app.chatView.addMessage, render: () => app.tui.requestRender(),
    resync: (msg?: string) => app.syncTranscript(msg),
    doResume: () => app.resume(),
    doFork: (entryId: string) => app.fork(entryId),
    setEditorText: (t: string) => app.editor.setText(t),
    getActiveOverlay: () => app.activeOverlay,
    setActiveOverlay: (o: { hide(): void } | null) => { app.activeOverlay = o; },
  };
  return {
    host: app.host,
    get model() { return { provider: app.currentModel.provider, id: app.currentModel.id, name: app.currentModel.name }; },
    sessionName: app.sessionName,
    setSessionName: (n: string | undefined) => { app.sessionName = n; },
    get transcriptLength() { return app.transcript.length; },
    msg: app.chatView.addMessage,
    render: () => app.tui.requestRender(),
    refreshHeader: () => app.updateHeader(),
    refreshFooter: () => app.updateFooter(),
    resync: (m?: string) => app.syncTranscript(m),
    doResume: () => app.resume(),
    doNewSession: () => app.newSession(),
    doTreeSelector: () => openTreeSelector(oc),
    doForkSelector: () => openForkSelector(oc),
    doClone: () => app.clone(),
    doFork: (eid: string) => app.fork(eid),
    doResumeSelector: () => openResumeSelector(oc),
    doModelSelector: async () => {
      const sel = await openModelSelector(oc, app.getModelList() as any);
      if (sel) { app.applyModelChange(sel); app.chatView.addMessage("system", `Switched to ${sel.model.provider}/${sel.model.id}`); app.updateHeader(); app.updateFooter(); app.chatView.rebuildChat(); app.tui.requestRender(); }
    },
    doModelScopeSelector: async () => {
      let sm: SettingsManager = app.opts.settingsManager!;
      if (!sm) { const { SettingsManager: SM } = await import("piko-host-runtime"); sm = SM.create(app.host.cwd); }
      await openModelScopeSelector(oc, sm);
      if (app.opts.modelRegistry) {
        app.opts.modelRegistry.setScopedModels(sm.getEnabledModels() ?? []);
        const scoped = app.opts.modelRegistry.listScopedModels();
        if (scoped.length > 0 && !scoped.some((m: any) => m.provider === app.currentModel.provider && m.id === app.currentModel.id)) {
          const r = app.opts.modelRegistry.resolve(scoped[0].id, scoped[0].provider);
          if (r) app.applyModelChange(r);
        }
      }
    },
    cycleModelForward: () => app.cycleModel(true),
    cycleModelBackward: () => app.cycleModel(false),
    thinkingLevel: app.currentThinkingLevel,
    setThinkingLevel: (l: string) => { app.currentThinkingLevel = l; app.host.setThinkingLevel(l); app.chatView.addMessage("system", `Thinking level: ${l}`); app.chatView.rebuildChat(); app.tui.requestRender(); },
    doThinkingSelector: async () => {
      const l = await openThinkingSelector(oc, app.currentThinkingLevel);
      if (l) { app.currentThinkingLevel = l; app.host.setThinkingLevel(l); app.chatView.addMessage("system", `Thinking level: ${l}`); app.chatView.rebuildChat(); app.tui.requestRender(); }
    },
    doLoginSelector: async (p: string) => {
      const saved = await openLoginDialog(oc, p); if (!saved) return;
      if (app.opts.modelRegistry) { const r = app.opts.modelRegistry.resolve(app.currentModel.id, app.currentModel.provider); if (r) app.applyModelChange(r); }
      else { const f = findModel(app.currentModel.id, app.currentModel.provider); if (f) app.applyModelChange({ model: f.model, providerConfig: f.providerConfig }); }
      app.chatView.addMessage("system", `Logged into ${p}. Config refreshed.`); app.chatView.rebuildChat(); app.tui.requestRender();
    },
    doSettingsSelector: async () => {
      let sm: SettingsManager = app.opts.settingsManager!;
      if (!sm) { const { SettingsManager: SM } = await import("piko-host-runtime"); sm = SM.create(app.host.cwd); }
      await openSettingsSelector(oc, sm); sm.reload();
      const nt = sm.getDefaultThinkingLevel(); if (nt && nt !== app.currentThinkingLevel) { app.currentThinkingLevel = nt; app.host.setThinkingLevel(nt); }
      const theme = sm.getTheme(); if (theme) { const m = getThemeManager(); if (m.switchTo(theme)) setTheme(m.get()); }
    },
    setEditorText: (t: string) => app.editor.setText(t),
    submitUserMessage: (t: string) => { app.editor.setText(""); app.submit(t); },
    submitStream: (f: any, label: string) => app.submitStream(f, label),
    switchTheme: (n: string) => { const m = getThemeManager(); const ok = m.switchTo(n); if (ok) setTheme(m.get()); return ok; },
    currentTheme: getThemeManager().getCurrentName(),
    reloadRuntime: async () => {
      app.opts.settingsManager?.reload();
      const nt = app.opts.settingsManager?.getDefaultThinkingLevel(); if (nt) { app.currentThinkingLevel = nt; app.host.setThinkingLevel(nt); }
      if (app.opts.modelRegistry) app.opts.modelRegistry.setScopedModels(app.opts.settingsManager?.getEnabledModels() ?? []);
      getThemeManager().load(app.host.cwd);
      const t = app.opts.settingsManager?.getTheme(); if (t) { const m = getThemeManager(); if (m.switchTo(t)) setTheme(m.get()); }
      await app.syncTranscript();
    },
    listModels: listAvailableModels,
    formatSessions: formatSessionTreeLines,
  };
}
