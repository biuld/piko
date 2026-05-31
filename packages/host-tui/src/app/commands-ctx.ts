import type { Editor } from "@earendil-works/pi-tui";
import {
  findModel,
  listAvailableModels,
  type SettingsManager,
} from "piko-host-runtime";
import type { CommandContext } from "../commands/index.js";
import {
  openForkSelector,
  openLoginDialog,
  openModelScopeSelector,
  openModelSelector,
  openResumeSelector,
  openSettingsSelector,
  openThinkingSelector,
  openTreeSelector,
  type OverlayContext,
} from "../overlays/index.js";
import { formatSessionTreeLines } from "../session-tree.js";
import { getThemeManager } from "../theme/index.js";
import { setTheme } from "../theme.js";
import type { TuiContext } from "./context.js";

export function buildCommandContext(
  ctx: TuiContext,
  editor: Editor,
  getEditorText: () => string,
): CommandContext {
  const overlayCtx: OverlayContext = {
    tui: ctx.tui,
    host: ctx.host,
    msg: ctx.chatView.addMessage,
    render: () => ctx.tui.requestRender(),
    resync: ctx.syncSessionTranscript,
    doResume: ctx.resumeSession,
    doFork: (entryId: string) => ctx.forkSessionCmd(entryId, (t: string) => editor.setText(t)),
    setEditorText: (text: string) => editor.setText(text),
    getActiveOverlay: () => ctx.activeOverlay,
    setActiveOverlay: (o) => { ctx.activeOverlay = o; },
  };

  const modelOps = ctx.modelOps;

  return {
    host: ctx.host,
    get model() { return { provider: ctx.currentModel.provider, id: ctx.currentModel.id, name: ctx.currentModel.name }; },
    sessionName: ctx.sessionName,
    setSessionName: (name: string | undefined) => { ctx.sessionName = name; },
    get transcriptLength() { return ctx.transcript.length; },
    msg: ctx.chatView.addMessage,
    render: () => ctx.tui.requestRender(),
    refreshHeader: ctx.updateHeader,
    refreshFooter: ctx.updateFooter,
    resync: ctx.syncSessionTranscript,
    doResume: ctx.resumeSession,
    doNewSession: ctx.createNewSession,
    doTreeSelector: () => openTreeSelector(overlayCtx),
    doForkSelector: () => openForkSelector(overlayCtx),
    doClone: ctx.cloneSessionCmd,
    doFork: (entryId: string) => ctx.forkSessionCmd(entryId, (t: string) => editor.setText(t)),
    doResumeSelector: () => openResumeSelector(overlayCtx),
    doModelSelector: async () => {
      const selected = await openModelSelector(overlayCtx, modelOps.getModelList());
      if (selected) {
        modelOps.applyModelChange(selected);
        ctx.chatView.addMessage("system", `Switched to ${selected.model.provider}/${selected.model.id}`);
        ctx.updateHeader();
        ctx.updateFooter();
        ctx.chatView.rebuildChat();
        ctx.tui.requestRender();
      }
    },
    doModelScopeSelector: async () => {
      let sm: SettingsManager = ctx.options.settingsManager!;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(ctx.host.cwd);
      }
      await openModelScopeSelector(overlayCtx, sm);
      if (ctx.options.modelRegistry) {
        const enabledModels = sm.getEnabledModels();
        ctx.options.modelRegistry.setScopedModels(enabledModels ?? []);
        const scoped = ctx.options.modelRegistry.listScopedModels();
        if (scoped.length > 0 && !scoped.some((m) => m.provider === ctx.currentModel.provider && m.id === ctx.currentModel.id)) {
          const resolved = ctx.options.modelRegistry.resolve(scoped[0].id, scoped[0].provider);
          if (resolved) modelOps.applyModelChange(resolved);
        }
      }
    },
    cycleModelForward: modelOps.cycleModelForward,
    cycleModelBackward: modelOps.cycleModelBackward,
    thinkingLevel: ctx.currentThinkingLevel,
    setThinkingLevel: (level: string) => {
      ctx.currentThinkingLevel = level;
      ctx.host.setThinkingLevel(level);
      ctx.chatView.addMessage("system", `Thinking level: ${level}`);
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
    },
    doThinkingSelector: async () => {
      const level = await openThinkingSelector(overlayCtx, ctx.currentThinkingLevel);
      if (level) {
        ctx.currentThinkingLevel = level;
        ctx.host.setThinkingLevel(level);
        ctx.chatView.addMessage("system", `Thinking level: ${level}`);
        ctx.chatView.rebuildChat();
        ctx.tui.requestRender();
      }
    },
    doLoginSelector: async (provider: string) => {
      const saved = await openLoginDialog(overlayCtx, provider);
      if (!saved) return;
      if (ctx.options.modelRegistry) {
        const resolved = ctx.options.modelRegistry.resolve(ctx.currentModel.id, ctx.currentModel.provider);
        if (resolved) modelOps.applyModelChange(resolved);
      } else {
        const found = findModel(ctx.currentModel.id, ctx.currentModel.provider);
        if (found) modelOps.applyModelChange({ model: found.model, providerConfig: found.providerConfig });
      }
      ctx.chatView.addMessage("system", `Logged into ${provider}. Config refreshed.`);
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
    },
    doSettingsSelector: async () => {
      let sm: SettingsManager = ctx.options.settingsManager!;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(ctx.host.cwd);
      }
      await openSettingsSelector(overlayCtx, sm);
      sm.reload();
      const newThinking = sm.getDefaultThinkingLevel();
      if (newThinking && newThinking !== ctx.currentThinkingLevel) {
        ctx.currentThinkingLevel = newThinking;
        ctx.host.setThinkingLevel(newThinking);
      }
      const newTheme = sm.getTheme();
      if (newTheme) {
        const manager = getThemeManager();
        if (manager.switchTo(newTheme)) setTheme(manager.get());
      }
    },
    setEditorText: (text: string) => editor.setText(text),
    submitUserMessage: (text: string) => { editor.setText(""); ctx.submitUserMessage(text); },
    submitStream: (factory, displayText) => {
      editor.setText("");
      ctx.running = true;
      ctx.abortController = new AbortController();
      const stream = factory(ctx.abortController.signal);
      ctx.spinner.start();
      if (ctx.workingIndicatorConfig) ctx.spinner.setIndicator(ctx.workingIndicatorConfig);
      ctx.statusLine.set("progress", "");
      ctx.chatView.addMessage("user", displayText);
      ctx.chatView.rebuildChat();
      ctx.tui.requestRender();
      ctx.extensionHost.dispatchEvent({ type: "message", role: "user", content: displayText });
      ctx.runStreamWithUI(stream, displayText);
    },
    switchTheme: (name: string) => {
      const manager = getThemeManager();
      const ok = manager.switchTo(name);
      if (ok) setTheme(manager.get());
      return ok;
    },
    currentTheme: getThemeManager().getCurrentName(),
    reloadRuntime: async () => {
      ctx.options.settingsManager?.reload();
      const newThinking = ctx.options.settingsManager?.getDefaultThinkingLevel();
      if (newThinking) { ctx.currentThinkingLevel = newThinking; ctx.host.setThinkingLevel(newThinking); }
      if (ctx.options.modelRegistry) {
        const enabledModels = ctx.options.settingsManager?.getEnabledModels();
        ctx.options.modelRegistry.setScopedModels(enabledModels ?? []);
      }
      getThemeManager().load(ctx.host.cwd);
      const settingsTheme = ctx.options.settingsManager?.getTheme();
      if (settingsTheme) {
        const switched = getThemeManager().switchTo(settingsTheme);
        if (switched) setTheme(getThemeManager().get());
      }
      await ctx.syncSessionTranscript();
    },
    listModels: listAvailableModels,
    formatSessions: formatSessionTreeLines,
  };
}
