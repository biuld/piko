import type { Component } from "@earendil-works/pi-tui";
import type { ResolvedModel } from "piko-host-runtime";
import { findModel, type SettingsManager } from "piko-host-runtime";
import type { CommandContext } from "../commands/index.js";
import {
  openForkSelector,
  openModelScopeSelector,
  openModelSelector,
  openResumeSelector,
  openSettingsSelector,
  openTreeSelector,
} from "../overlays/index.js";
import { getThemeManager } from "../theme/index.js";
import { setTheme } from "../theme.js";
import type { BaseApp } from "./base.js";
import { openLoginFlow, openLogoutFlow } from "./login-flow.js";

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
  submitStream(
    factory: (sig: AbortSignal) => any,
    label: string,
    kind?: "skill" | "template",
  ): void;
}

export function buildCommandContext(app: CommandsCtxDeps): CommandContext {
  const oc = {
    tui: app.tui,
    host: app.host,
    msg: app.chatView.addMessage,
    render: () => app.tui.requestRender(),
    resync: (msg?: string) => app.syncTranscript(msg),
    doResume: () => app.resume(),
    doFork: (entryId: string) => app.fork(entryId),
    setEditorText: (t: string) => app.editor.setText(t),
    showReplacement: (component: Component, focusTarget?: Component) =>
      app.showEditorReplacement(component, focusTarget),
    restoreEditor: () => app.restoreEditor(),
    getActiveOverlay: () => app.activeOverlay,
    setActiveOverlay: (o: { hide(): void } | null) => {
      app.activeOverlay = o;
    },
  };
  return {
    host: app.host,
    get model() {
      return {
        provider: app.currentModel.provider,
        id: app.currentModel.id,
        name: app.currentModel.name,
      };
    },
    sessionName: app.sessionName,
    setSessionName: (n: string | undefined) => {
      app.sessionName = n;
    },
    get transcriptLength() {
      return app.transcript.length;
    },
    msg: app.chatView.addMessage,
    render: () => app.tui.requestRender(),
    refreshHeader: () => app.updateHeader(),
    refreshFooter: () => app.updateFooter(),
    resync: (m?: string) => app.syncTranscript(m),
    doResume: () => app.resume(),
    shutdown: () => app.shutdown(),
    doNewSession: () => app.newSession(),
    doTreeSelector: () => openTreeSelector(oc),
    doForkSelector: () => openForkSelector(oc),
    doClone: () => app.clone(),
    doResumeSelector: () => openResumeSelector(oc),
    doModelSelector: async (search?: string) => {
      const allModels = app.getModelList() as any[];
      const scoped = app.opts.modelRegistry?.listScopedModels() ?? [];
      const scopedModels =
        scoped.length > 0
          ? scoped.map((m: any) => {
              const found = allModels.find(
                (a: any) => a.model.provider === m.provider && a.model.id === m.id,
              );
              return found ?? { model: m, providerConfig: app.currentProviderConfig };
            })
          : [];
      const sel = await openModelSelector(
        oc,
        allModels as any,
        scopedModels as any,
        app.currentModel,
        search,
      );
      if (sel) {
        app.applyModelChange(sel);
        app.chatView.addMessage("system", `Switched to ${sel.model.provider}/${sel.model.id}`);
        app.updateHeader();
        app.updateFooter();
        app.chatView.rebuildChat();
        app.tui.requestRender();
      }
    },
    doModelScopeSelector: async () => {
      let sm: SettingsManager = app.opts.settingsManager!;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(app.host.cwd);
      }
      await openModelScopeSelector(oc, sm);
      if (app.opts.modelRegistry) {
        app.opts.modelRegistry.setScopedModels(sm.getEnabledModels() ?? []);
        const scoped = app.opts.modelRegistry.listScopedModels();
        if (
          scoped.length > 0 &&
          !scoped.some(
            (m: any) => m.provider === app.currentModel.provider && m.id === app.currentModel.id,
          )
        ) {
          const r = app.opts.modelRegistry.resolve(scoped[0].id, scoped[0].provider);
          if (r) app.applyModelChange(r);
        }
      }
    },
    cycleModelForward: () => app.cycleModel(true),
    cycleModelBackward: () => app.cycleModel(false),
    thinkingLevel: app.currentThinkingLevel,
    setThinkingLevel: (l: string) => {
      app.currentThinkingLevel = l;
      app.host.setThinkingLevel(l);
      app.chatView.addMessage("system", `Thinking level: ${l}`);
      app.chatView.rebuildChat();
      app.tui.requestRender();
    },
    doLoginSelector: async () => {
      const saved = await openLoginFlow(oc, app.opts.authStorage);
      if (!saved) return;
      app.opts.authStorage?.reload();
      if (app.opts.modelRegistry) {
        // Re-resolve model with potentially new auth
        const r = app.opts.modelRegistry.resolve(app.currentModel.id, app.currentModel.provider);
        if (r) app.applyModelChange(r);
      } else {
        const f = findModel(app.currentModel.id, app.currentModel.provider);
        if (f) app.applyModelChange({ model: f.model, providerConfig: f.providerConfig });
      }
      app.chatView.addMessage("system", `Logged in. Config refreshed.`);
      app.chatView.rebuildChat();
      app.tui.requestRender();
    },
    doLogoutSelector: async () => {
      const saved = await openLogoutFlow(oc, app.opts.authStorage);
      if (!saved) return;
      app.opts.authStorage?.reload();
      if (app.opts.modelRegistry) {
        const r = app.opts.modelRegistry.resolve(app.currentModel.id, app.currentModel.provider);
        if (r) app.applyModelChange(r);
      }
      app.chatView.rebuildChat();
      app.tui.requestRender();
    },
    doSettingsSelector: async () => {
      let sm: SettingsManager = app.opts.settingsManager!;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(app.host.cwd);
      }
      await openSettingsSelector(oc, sm, {
        onThemePreview: (themeName: string) => {
          const m = getThemeManager();
          if (m.switchTo(themeName)) setTheme(m.get());
          app.tui.requestRender();
        },
        onThemeChange: (_themeName: string) => {
          // Already applied via preview, persist is handled by SettingsManager.setTheme
        },
      });
      sm.reload();

      // Apply thinking level
      const nt = sm.getDefaultThinkingLevel();
      if (nt && nt !== app.currentThinkingLevel) {
        app.currentThinkingLevel = nt;
        app.host.setThinkingLevel(nt);
      }

      // Apply theme
      const theme = sm.getTheme();
      if (theme) {
        const m = getThemeManager();
        if (m.switchTo(theme)) setTheme(m.get());
      }

      // Refresh model scope (scoped models may have changed)
      if (app.opts.modelRegistry) {
        const enabledModels = sm.getEnabledModels();
        if (enabledModels) {
          app.opts.modelRegistry.setScopedModels(enabledModels);
        }
      }

      // Signal that compaction/retry settings may have changed
      const compactionSettings = sm.getCompactionSettings();
      if (compactionSettings) {
        app.chatView.addMessage(
          "system",
          `Settings updated: compaction ${compactionSettings.enabled ? "enabled" : "disabled"}, reserve ${compactionSettings.reserveTokens} tokens`,
        );
      }

      app.updateHeader();
      app.chatView.rebuildChat();
      app.tui.requestRender();
    },
    submitStream: (f: any, label: string, kind?: "skill" | "template") =>
      app.submitStream(f, label, kind),
    reloadRuntime: async () => {
      app.opts.settingsManager?.reload();
      const nt = app.opts.settingsManager?.getDefaultThinkingLevel();
      if (nt) {
        app.currentThinkingLevel = nt;
        app.host.setThinkingLevel(nt);
      }
      if (app.opts.modelRegistry)
        app.opts.modelRegistry.setScopedModels(app.opts.settingsManager?.getEnabledModels() ?? []);
      getThemeManager().load(app.host.cwd);
      const t = app.opts.settingsManager?.getTheme();
      if (t) {
        const m = getThemeManager();
        if (m.switchTo(t)) setTheme(m.get());
      }
      await app.syncTranscript();
    },
  };
}
