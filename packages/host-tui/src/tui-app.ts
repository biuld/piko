import type { Model } from "@earendil-works/pi-ai";
import {
  type AutocompleteProvider,
  type AutocompleteSuggestions,
  Box,
  type Component,
  Editor,
  type EditorComponent,
  type LoaderIndicatorOptions,
  ProcessTerminal,
  Spacer,
  Text,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { AuthStorage } from "piko-host-runtime";
import {
  computeCumulativeUsage,
  createDefaultSettings,
  createHostConfig,
  findModel,
  getContextPercent,
  listAvailableModels,
  type ModelRegistry,
  PikoHost,
  processFileArguments,
  type ResolvedModel,
  type SettingsManager,
} from "piko-host-runtime";
import { InteractiveApprovalHandler } from "./approval-handler.js";
import { ChatView } from "./chat-view.js";
import { COMMANDS, type CommandContext, handleSlashCommand } from "./commands/index.js";
import { DynamicBorder } from "./components/dynamic-border.js";
import { FooterComponent } from "./components/footer.js";
import { getContextHints } from "./components/key-hints.js";
import { Spinner } from "./components/spinner.js";
import { StatusLine } from "./components/status-line.js";
import { WidgetSlot } from "./components/widget-slot.js";
import {
  type EditorFactory,
  ExtensionHost,
  type FooterFactory,
  type PikoExtensionFactory,
  type WidgetContent,
  type WidgetPlacement,
} from "./extensions/index.js";
import {
  type OverlayContext,
  openForkSelector,
  openLoginDialog,
  openModelScopeSelector,
  openModelSelector,
  openResumeSelector,
  openSettingsSelector,
  openThinkingSelector,
  openTreeSelector,
} from "./overlays/index.js";
import { formatSessionTreeLines } from "./session-tree.js";
import { getThemeManager } from "./theme/manager.js";
import { getEditorTheme, getTheme, setTheme } from "./theme.js";

export interface RunTuiOptions {
  session?: string;
  extensions?: PikoExtensionFactory[];
  /** Settings manager for layered configuration. */
  settingsManager?: SettingsManager;
  /** Model registry (with auth integration) for model resolution and cycling. */
  modelRegistry?: ModelRegistry;
  /** Auth storage for login/logout. */
  authStorage?: AuthStorage;
  /** Initial session name. */
  sessionName?: string;
  /** Skip loading AGENTS.md / CLAUDE.md context files. */
  noContextFiles?: boolean;
  /** Disable tool calling. */
  noTools?: boolean;
  /** Custom system prompt (replaces default). */
  systemPrompt?: string;
  /** Append to default system prompt. */
  appendSystemPrompt?: string;
}

function createAutocomplete(extensionHost?: ExtensionHost): AutocompleteProvider {
  return {
    async getSuggestions(
      lines: string[],
      cursorLine: number,
      cursorCol: number,
    ): Promise<AutocompleteSuggestions | null> {
      const line = lines[cursorLine] ?? "";
      const prefix = line.slice(0, cursorCol);
      if (!prefix.startsWith("/")) return null;

      const allCommands = [
        ...COMMANDS,
        ...(extensionHost?.commands.map((c) => ({
          value: c.value,
          label: c.label,
          description: c.description,
        })) ?? []),
      ];
      return {
        items: allCommands.filter((c) => c.value.startsWith(prefix)),
        prefix: "/",
      };
    },
    applyCompletion(
      lines: string[],
      cursorLine: number,
      _cursorCol: number,
      item: { value: string; label: string },
      prefix: string,
    ) {
      const line = lines[cursorLine] ?? "";
      const slashIdx = line.indexOf(prefix);
      const before = line.slice(0, slashIdx);
      const newLine = `${before + item.value} `;
      return { lines: [newLine], cursorLine, cursorCol: newLine.length };
    },
  };
}

function makeHostOptions(
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  sessionOptions: { session?: string },
  settingsManager?: SettingsManager,
  tuiOptions?: RunTuiOptions,
): Parameters<typeof PikoHost.create>[0] {
  return {
    config: createHostConfig(
      model,
      providerConfig,
      createDefaultSettings({
        maxSteps: 10,
        parallelTools: false,
        allowToolCalls: tuiOptions?.noTools ? false : true,
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

export async function runTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);
  const theme = getTheme();

  // Mutable model state (for cycling) — updated via host.setConfig()
  let currentModel = initialModel;
  let currentProviderConfig = initialProviderConfig;
  /** Unified turn config: model, thinking level, etc. Always kept in sync with host. */
  let currentThinkingLevel: string = options.settingsManager?.getDefaultThinkingLevel() ?? "off";

  // ---- Extension host ----
  const extensionHost = new ExtensionHost({
    tui,
    theme,
    setEditorText: () => {},
    getEditorText: () => "",
    addChatMessage: () => {},
    requestRender: () => tui.requestRender(),
    setFooterFactory: () => {},
    setEditorFactory: () => {},
    setWidgetSlot: () => {},
    setStatusSlot: () => {},
    setWorkingIndicatorConfig: () => {},
  });

  // ---- Load extensions (before host, so custom tools can be registered) ----
  if (options.extensions?.length) {
    await extensionHost.loadAll(options.extensions);
  }

  // ---- Host ----
  const host = await PikoHost.create({
    ...makeHostOptions(
      currentModel,
      currentProviderConfig,
      { session: options.session },
      options.settingsManager,
      options,
    ),
    approvalHandler: new InteractiveApprovalHandler(tui),
    customTools: extensionHost.tools.length > 0 ? extensionHost.tools : undefined,
  });

  // Load external themes from .piko/themes/
  getThemeManager().load(host.cwd);

  // Apply theme from settings if available
  const settingsTheme = options.settingsManager?.getTheme();
  if (settingsTheme) {
    const switched = getThemeManager().switchTo(settingsTheme);
    if (switched) setTheme(getThemeManager().get());
  }

  let transcript = await host.loadMessages();
  let sessionName = await host.getSessionName();

  // Apply initial session name from CLI
  if (options.sessionName && !sessionName) {
    await host.setSessionName(options.sessionName);
    sessionName = options.sessionName;
  }

  let running = false;
  let abortController: AbortController | null = null;
  let activeOverlay: { hide(): void } | null = null;
  let cumulativeInput = 0;
  let cumulativeOutput = 0;
  let cumulativeCacheRead = 0;
  let cumulativeCacheWrite = 0;
  let cumulativeCost = 0;

  // ---- Chat ----
  const chatBox = new Box(0, 0);
  const chatView = new ChatView(chatBox);

  // ---- Widget slots ----
  const widgetSlotAbove = new WidgetSlot();
  const widgetSlotBelow = new WidgetSlot();
  widgetSlotAbove.bind(tui);
  widgetSlotBelow.bind(tui);

  // ---- Footer ----
  let customFooterFactory: FooterFactory | undefined;
  const footerComponent = new FooterComponent({
    model: currentModel,
    sessionName,
    messageCount: transcript.length,
    cwd: host.cwd,
  });

  function getFooterComponent(): Component {
    if (customFooterFactory) return customFooterFactory(tui, theme);
    return footerComponent;
  }

  // ---- Editor ----
  let customEditorFactory: EditorFactory | undefined;
  const editor = new Editor(tui, getEditorTheme());
  editor.setAutocompleteProvider(createAutocomplete(extensionHost));

  function getEditorComponent(): EditorComponent {
    if (customEditorFactory) {
      const customEditor = customEditorFactory(tui, theme);
      customEditor.onSubmit = (text: string) => editor.onSubmit?.(text);
      return customEditor;
    }
    return editor;
  }

  // ---- Status line ----
  const statusLine = new StatusLine();

  // ---- Spinner ----
  const spinner = new Spinner();
  spinner.bind(tui);

  let workingIndicatorConfig: LoaderIndicatorOptions | undefined;

  // ---- Bind extension host ----
  extensionHost.bindRuntime({
    setEditorText: (text: string) => editor.setText(text),
    getEditorText: () => getEditorComponent().getText(),
    addChatMessage: (role: string, text: string) => chatView.addMessage(role, text),
    setFooterFactory: (f: FooterFactory | undefined) => {
      customFooterFactory = f;
      tui.requestRender();
    },
    setEditorFactory: (f: EditorFactory | undefined) => {
      customEditorFactory = f;
      tui.requestRender();
    },
    setWidgetSlot: (
      key: string,
      content: WidgetContent | undefined,
      placement: WidgetPlacement,
    ) => {
      if (placement === "belowEditor") {
        widgetSlotBelow.set(key, content);
      } else {
        widgetSlotAbove.set(key, content);
      }
      tui.requestRender();
    },
    setStatusSlot: (key: string, text: string | undefined) => {
      statusLine.set(key, text);
      updateFooter();
      tui.requestRender();
    },
    setWorkingIndicatorConfig: (config?: LoaderIndicatorOptions) => {
      workingIndicatorConfig = config;
      if (spinner.active) spinner.setIndicator(config);
    },
  });

  host.onAfterRebind(async () => {
    await host.restoreFromSession();
    await syncSessionTranscript();
  });

  function updateHeader(): void {
    headerBox.clear();
    const t = getTheme();
    headerBox.addChild(new DynamicBorder((s) => t.fg("border", s)));
    const headerText = ` piko  ${currentModel.provider}/${currentModel.id}  session ${sessionName ?? host.sessionId.slice(-8)}  ${transcript.length} msgs `;
    headerBox.addChild(new Text(t.fg("accent", headerText), 1, 0));
    headerBox.addChild(new DynamicBorder((s) => t.fg("border", s)));
  }

  function updateFooter(): void {
    const statuses = [...statusLine.getEntries()];
    const keyHints = running
      ? getContextHints("streaming")
      : activeOverlay
        ? getContextHints("overlay")
        : getContextHints("normal");
    footerComponent.update({
      model: currentModel,
      sessionName,
      messageCount: transcript.length,
      cwd: host.cwd,
      totalInputTokens: cumulativeInput || undefined,
      totalOutputTokens: cumulativeOutput || undefined,
      totalCacheRead: cumulativeCacheRead || undefined,
      totalCacheWrite: cumulativeCacheWrite || undefined,
      totalCost: cumulativeCost || undefined,
      contextWindow: (currentModel as { contextWindow?: number }).contextWindow,
      contextPercent: (currentModel as { contextWindow?: number }).contextWindow
        ? getContextPercent(
            cumulativeInput,
            (currentModel as { contextWindow?: number }).contextWindow!,
          )
        : undefined,
      extensionStatuses: statuses.length > 0 ? statuses : undefined,
      keyHints: keyHints || undefined,
    });
  }

  async function syncSessionTranscript(systemMessage?: string): Promise<void> {
    const loaded = await host.loadMessages();
    sessionName = await host.getSessionName();
    transcript = [...loaded];
    updateHeader();
    updateFooter();
    chatView.rebuildFromTranscript(transcript, systemMessage);
    chatView.rebuildChat();
    tui.requestRender();
  }

  async function resumeSession(): Promise<void> {
    const loaded = await host.loadMessages();
    if (loaded.length === 0) {
      chatView.addMessage("system", `Session ${host.sessionId} not found or empty`);
      chatView.rebuildChat();
      tui.requestRender();
      return;
    }
    // Restore model/thinking state from session entries
    await host.restoreFromSession();
    // Sync TUI state with restored host config
    currentModel = host.getConfig().model;
    currentProviderConfig = host.getConfig().provider;
    currentThinkingLevel = host.getThinkingLevel();
    chatView.addMessage("system", `Resumed session ${host.sessionId} (${loaded.length} messages)`);
    chatView.rebuildChat();
    tui.requestRender();
  }

  async function createNewSession(): Promise<void> {
    await host.newSession();
    chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    chatView.rebuildChat();
    tui.requestRender();
  }

  async function cloneSessionCmd(): Promise<void> {
    if (!host.isSessionPersisted()) {
      chatView.addMessage("system", "Clone requires a saved session");
      chatView.rebuildChat();
      tui.requestRender();
      return;
    }
    await host.cloneSession();
    chatView.addMessage("system", `Cloned branch into session ${host.sessionId}`);
    chatView.rebuildChat();
    tui.requestRender();
  }

  async function forkSessionCmd(entryId: string): Promise<void> {
    if (!host.isSessionPersisted()) {
      chatView.addMessage("system", "Fork requires a saved session");
      chatView.rebuildChat();
      tui.requestRender();
      return;
    }
    const result = await host.forkSession(entryId);
    const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
    chatView.addMessage("system", `Forked into session ${host.sessionId}${suffix}`);
    chatView.rebuildChat();
    tui.requestRender();
    if (result.selectedText) editor.setText(result.selectedText);
  }

  const overlayCtx: OverlayContext = {
    tui,
    host,
    msg: chatView.addMessage,
    render: () => tui.requestRender(),
    resync: syncSessionTranscript,
    doResume: resumeSession,
    doFork: forkSessionCmd,
    setEditorText: (text) => editor.setText(text),
    getActiveOverlay: () => activeOverlay,
    setActiveOverlay: (o) => {
      activeOverlay = o;
    },
  };

  const headerBox = new Box(0, 0);
  updateHeader();

  // Model list for selector and cycling
  // Each entry gets its own resolved providerConfig (fix #2)
  function getModelList(): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
    if (options.modelRegistry) {
      const registryModels = options.modelRegistry.listScopedModels();
      return registryModels.map((m) => {
        const resolved = options.modelRegistry!.resolve(m.id, m.provider);
        return {
          model: m,
          providerConfig: resolved?.providerConfig ?? currentProviderConfig,
        };
      });
    }
    const allProviders = listAvailableModels();
    return allProviders.flatMap((p) =>
      p.models.map((m) => {
        const found = findModel(m.id, p.provider);
        return {
          model: { provider: p.provider, id: m.id, name: m.name } as Model<string>,
          providerConfig: found?.providerConfig ?? currentProviderConfig,
        };
      }),
    );
  }

  function getModelIds(): string[] {
    if (options.modelRegistry) {
      return options.modelRegistry.listScopedModels().map((m) => `${m.provider}/${m.id}`);
    }
    const allProviders = listAvailableModels();
    return allProviders.flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`));
  }

  function resolveModel(id: string, prov: string): ResolvedModel | null {
    if (options.modelRegistry) {
      return options.modelRegistry.resolve(id, prov);
    }
    const found = findModel(id, prov);
    if (found) {
      return { model: found.model, providerConfig: found.providerConfig };
    }
    return null;
  }

  const allModelIds = getModelIds();

  /** Update host config + local state when model changes. */
  function applyModelChange(found: ResolvedModel): void {
    currentModel = found.model;
    currentProviderConfig = found.providerConfig;
    host.setConfig(
      createHostConfig(
        found.model,
        found.providerConfig,
        createDefaultSettings({
          maxSteps: 10,
          parallelTools: false,
          allowToolCalls: options.noTools ? false : true,
          allowApprovals: true,
        }),
      ),
    );
    host.setThinkingLevel(currentThinkingLevel);
  }

  async function cycleModelForward() {
    const currentId = `${currentModel.provider}/${currentModel.id}`;
    const modelIds = getModelIds();
    const currentIdx = modelIds.indexOf(currentId);
    if (currentIdx === -1 || modelIds.length === 0) return;
    const nextIdx = (currentIdx + 1) % modelIds.length;
    const nextId = modelIds[nextIdx];
    const [prov, id] = nextId.split("/");
    const found = resolveModel(id, prov);
    if (found) {
      applyModelChange(found);
      chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      updateHeader();
      updateFooter();
      chatView.rebuildChat();
      tui.requestRender();
    }
  }

  async function cycleModelBackward() {
    const currentId = `${currentModel.provider}/${currentModel.id}`;
    const modelIds = getModelIds();
    const currentIdx = modelIds.indexOf(currentId);
    if (currentIdx === -1 || modelIds.length === 0) return;
    const prevIdx = (currentIdx - 1 + modelIds.length) % modelIds.length;
    const prevId = modelIds[prevIdx];
    const [prov, id] = prevId.split("/");
    const found = resolveModel(id, prov);
    if (found) {
      applyModelChange(found);
      chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      updateHeader();
      updateFooter();
      chatView.rebuildChat();
      tui.requestRender();
    }
  }

  const cmdCtx: CommandContext = {
    host,
    get model() { return { provider: currentModel.provider, id: currentModel.id, name: currentModel.name }; },
    sessionName,
    setSessionName: (name: string | undefined) => {
      sessionName = name;
    },
    get transcriptLength() { return transcript.length; },
    msg: chatView.addMessage,
    render: () => tui.requestRender(),
    refreshHeader: updateHeader,
    refreshFooter: updateFooter,
    resync: syncSessionTranscript,
    doResume: resumeSession,
    doNewSession: createNewSession,
    doTreeSelector: () => openTreeSelector(overlayCtx),
    doForkSelector: () => openForkSelector(overlayCtx),
    doClone: cloneSessionCmd,
    doFork: forkSessionCmd,
    doResumeSelector: () => openResumeSelector(overlayCtx),
    doModelSelector: async () => {
      // Recompute model list every time (fix #2 — picks up auth/scope changes)
      const selected = await openModelSelector(overlayCtx, getModelList());
      if (selected) {
        applyModelChange(selected);
        chatView.addMessage(
          "system",
          `Switched to ${selected.model.provider}/${selected.model.id}`,
        );
        updateHeader();
        updateFooter();
        chatView.rebuildChat();
        tui.requestRender();
      }
    },
    doModelScopeSelector: async () => {
      let sm = options.settingsManager;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(host.cwd);
      }
      await openModelScopeSelector(overlayCtx, sm);
      // Reload ModelRegistry scope and refresh host config
      if (options.modelRegistry) {
        const enabledModels = sm.getEnabledModels();
        options.modelRegistry.setScopedModels(enabledModels ?? []);
        const scoped = options.modelRegistry.listScopedModels();
        if (scoped.length > 0 && !scoped.some((m) => m.provider === currentModel.provider && m.id === currentModel.id)) {
          // Current model no longer in scope — switch to first available
          const resolved = options.modelRegistry.resolve(scoped[0].id, scoped[0].provider);
          if (resolved) applyModelChange(resolved);
        }
      }
    },
    cycleModelForward,
    cycleModelBackward,
    thinkingLevel: currentThinkingLevel,
    setThinkingLevel: (level: string) => {
      currentThinkingLevel = level;
      host.setThinkingLevel(level);
      chatView.addMessage("system", `Thinking level: ${level}`);
      chatView.rebuildChat();
      tui.requestRender();
    },
    doThinkingSelector: async () => {
      const level = await openThinkingSelector(overlayCtx, currentThinkingLevel);
      if (level) {
        currentThinkingLevel = level;
        host.setThinkingLevel(level);
        chatView.addMessage("system", `Thinking level: ${level}`);
        chatView.rebuildChat();
        tui.requestRender();
      }
    },
    doLoginSelector: async (provider: string) => {
      const saved = await openLoginDialog(overlayCtx, provider);
      if (!saved) return; // User cancelled — don't refresh (fix #1)
      // Re-resolve current model's provider config with new credentials
      if (options.modelRegistry) {
        const resolved = options.modelRegistry.resolve(currentModel.id, currentModel.provider);
        if (resolved) {
          currentProviderConfig = resolved.providerConfig;
          host.setConfig(
            createHostConfig(
              currentModel,
              currentProviderConfig,
              createDefaultSettings({
                maxSteps: 10,
                parallelTools: false,
                allowToolCalls: options.noTools ? false : true,
                allowApprovals: true,
              }),
            ),
          );
        }
      } else {
        const found = findModel(currentModel.id, currentModel.provider);
        if (found) {
          currentProviderConfig = found.providerConfig;
          host.setConfig(
            createHostConfig(
              currentModel,
              currentProviderConfig,
              createDefaultSettings({
                maxSteps: 10,
                parallelTools: false,
                allowToolCalls: options.noTools ? false : true,
                allowApprovals: true,
              }),
            ),
          );
        }
      }
      chatView.addMessage("system", `Logged into ${provider}. Config refreshed.`);
      chatView.rebuildChat();
      tui.requestRender();
    },
    doSettingsSelector: async () => {
      let sm = options.settingsManager;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(host.cwd);
      }
      await openSettingsSelector(overlayCtx, sm);
      // Reload settings and apply to runtime
      sm.reload();
      const newThinking = sm.getDefaultThinkingLevel();
      if (newThinking && newThinking !== currentThinkingLevel) {
        currentThinkingLevel = newThinking;
        host.setThinkingLevel(newThinking);
      }
      const newTheme = sm.getTheme();
      if (newTheme) {
        const manager = getThemeManager();
        if (manager.switchTo(newTheme)) setTheme(manager.get());
      }
    },
    setEditorText: (text: string) => {
      editor.setText(text);
    },
    submitUserMessage: (text: string) => {
      // Clear editor and submit as if the user typed this (fix #5)
      editor.setText("");
      submitUserMessage(text);
    },
    /** Submit a stream from host APIs via factory(fn(signal) => stream). Signal is passed for Ctrl+C abort (fix #1). */
    submitStream: (factory: (signal: AbortSignal) => ReturnType<typeof host.streamPrompt>, displayText: string) => {
      editor.setText("");
      running = true;
      abortController = new AbortController();
      const stream = factory(abortController.signal);
      spinner.start();
      if (workingIndicatorConfig) spinner.setIndicator(workingIndicatorConfig);
      statusLine.set("progress", "");
      chatView.addMessage("user", displayText);
      chatView.rebuildChat();
      tui.requestRender();
      extensionHost.dispatchEvent({ type: "message", role: "user", content: displayText });
      runStreamWithUI(stream, displayText);
    },
    switchTheme: (name: string) => {
      const manager = getThemeManager();
      const ok = manager.switchTo(name);
      if (ok) setTheme(manager.get());
      return ok;
    },
    currentTheme: getThemeManager().getCurrentName(),
    reloadRuntime: async () => {
      // Reload settings
      options.settingsManager?.reload();
      const newThinking = options.settingsManager?.getDefaultThinkingLevel();
      if (newThinking) {
        currentThinkingLevel = newThinking;
        host.setThinkingLevel(newThinking);
      }
      // Reload model scope
      if (options.modelRegistry) {
        const enabledModels = options.settingsManager?.getEnabledModels();
        options.modelRegistry.setScopedModels(enabledModels ?? []);
      }
      // Reload theme
      getThemeManager().load(host.cwd);
      const settingsTheme = options.settingsManager?.getTheme();
      if (settingsTheme) {
        const switched = getThemeManager().switchTo(settingsTheme);
        if (switched) setTheme(getThemeManager().get());
      }
      // Reload skills and templates (reload host resources? already in constructor)
      // Resync transcript to pick up any changes
      await syncSessionTranscript();
    },
    listModels: listAvailableModels,
    formatSessions: formatSessionTreeLines,
  };

  // ---- Editor submit handler ----
  /* Editor submit handler: check extensions, slash commands, then delegate to submitUserMessage. */
  editor.onSubmit = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;

    // When running, queue as steering message instead of blocking (agent loop parity)
    if (running) {
      host.steer(trimmed);
      chatView.addMessage("system", `Queued for next turn: ${trimmed.slice(0, 80)}${trimmed.length > 80 ? "..." : ""}`);
      chatView.rebuildChat();
      tui.requestRender();
      return;
    }

    const extCmd = extensionHost.findCommand(trimmed);
    if (extCmd) {
      extCmd.handler(trimmed.slice(extCmd.value.length).trim(), {
        theme,
        setEditorText: (t: string) => editor.setText(t),
        getEditorText: () => getEditorComponent().getText(),
      } as any);
      return;
    }

    if (trimmed.startsWith("/")) {
      handleSlashCommand(trimmed, cmdCtx);
      return;
    }

    submitUserMessage(trimmed);
  };

  /**
   * Run a pre-built EventStream through the TUI rendering pipeline.
   * Used by both normal user messages and host API invocations (fix #3).
   */
  function runStreamWithUI(stream: ReturnType<typeof host.streamPrompt>, displayText: string): void {
    let hasAssistant = false;
    const toolCallIds: Map<string, string> = new Map();
    const toolCallNames: Map<string, string> = new Map();

    void (async () => {
      for await (const event of stream) {
        if (event.type === "message_delta") {
          if (!hasAssistant) {
            chatView.addMessage("assistant", (event as { delta: string }).delta);
            hasAssistant = true;
          } else {
            chatView.updateLastAssistant((event as { delta: string }).delta);
          }
          chatView.rebuildChat();
          tui.requestRender();
        } else if (event.type === "thinking_delta") {
          statusLine.set("progress", theme.fg("muted", "Thinking..."));
          tui.requestRender();
        } else if (event.type === "tool_call_start") {
          statusLine.set("progress", theme.fg("toolPendingBg", `Running ${event.name}...`));
          const tid = chatView.startToolCall(event.name, event.args, host.cwd);
          toolCallIds.set(event.id, tid);
          toolCallNames.set(event.id, event.name);
          chatView.rebuildChat();
          tui.requestRender();
          extensionHost.dispatchEvent({
            type: "tool_call_start",
            name: event.name,
            args: event.args as Record<string, unknown>,
          });
        } else if (event.type === "tool_call_end") {
          const toolName = toolCallNames.get(event.id) ?? "tool";
          statusLine.set(
            "progress",
            event.isError ? theme.fg("error", `${toolName} failed`) : theme.fg("success", `${toolName} completed`),
          );
          const tid = toolCallIds.get(event.id);
          if (tid) chatView.endToolCall(tid, event.result, event.isError);
          chatView.rebuildChat();
          tui.requestRender();
          extensionHost.dispatchEvent({ type: "tool_call_end", name: toolName, result: event.result, isError: event.isError });
        }
      }

      const result = await stream.result();
      spinner.stop();
      abortController = null;
      transcript = result.messages;
      const usage = computeCumulativeUsage(result.messages);
      cumulativeInput += usage.input;
      cumulativeOutput += usage.output;
      cumulativeCacheRead += usage.cacheRead;
      cumulativeCacheWrite += usage.cacheWrite;
      cumulativeCost += usage.cost;
      chatView.rebuildFromTranscript(
        transcript,
        result.status === "max_steps"
          ? "Stopped after reaching max steps"
          : result.status === "aborted"
            ? "Interrupted"
            : result.status === "error"
              ? "Run failed"
              : undefined,
      );
      updateHeader();
      updateFooter();
      statusLine.set("progress", undefined);
      running = false;
      chatView.rebuildChat();
      tui.requestRender();
      extensionHost.dispatchEvent({
        type: "turn_end",
        status: result.status,
        steps: transcript.length,
      });
    })().catch((error: unknown) => {
      spinner.stop();
      abortController = null;
      running = false;
      const message = error instanceof Error ? error.message : String(error);
      chatView.addMessage("system", message);
      chatView.rebuildChat();
      tui.requestRender();
    });
  }

  /** Submit a user message — separated from editor.onSubmit so commands can trigger it (fix #5). */
  function submitUserMessage(text: string): void {
    const trimmed = text.trim();
    if (!trimmed) return;

    // Expand @file references in the user message
    const { expanded: expandedText } = processFileArguments(trimmed, host.cwd);

    running = true;
    abortController = new AbortController();
    spinner.start();
    if (workingIndicatorConfig) spinner.setIndicator(workingIndicatorConfig);
    statusLine.set("progress", "");
    chatView.addMessage("user", expandedText);
    chatView.rebuildChat();
    tui.requestRender();

    extensionHost.dispatchEvent({ type: "message", role: "user", content: expandedText });

    const stream = host.streamPrompt(expandedText, {}, abortController.signal);
    runStreamWithUI(stream, expandedText);
  }

  // ---- Layout ----
  tui.addChild(headerBox);
  tui.addChild(chatBox);
  tui.addChild(widgetSlotAbove);
  tui.addChild(spinner);
  tui.addChild(statusLine);
  tui.addChild(new Spacer(1));
  tui.addChild(widgetSlotBelow);
  tui.addChild(new DynamicBorder((s) => theme.fg("borderMuted", s)));
  tui.addChild(editor);
  tui.setFocus(editor);
  tui.addChild(getFooterComponent());

  // ---- Global keyboard shortcuts + clipboard image paste ----
  let pasteBuffer = "";
  let isImagePaste = false;

  tui.addInputListener((data: string) => {
    if (running) return undefined;

    // Detect bracketed paste start
    if (data.includes("\x1b[200~")) {
      pasteBuffer = data.replace("\x1b[200~", "");
      isImagePaste = false;
      return undefined;
    }

    // Detect bracketed paste end — check for image data
    if (data.includes("\x1b[201~")) {
      const endIdx = data.indexOf("\x1b[201~");
      pasteBuffer += data.slice(0, endIdx);

      // Check if the pasted data looks like an image
      if (pasteBuffer.length > 100 && isImageData(Buffer.from(pasteBuffer, "binary"))) {
        isImagePaste = true;
        // Process image paste — save to temp, insert @path in editor
        handleImagePaste(Buffer.from(pasteBuffer, "binary"));
        pasteBuffer = "";
        return { consume: true };
      }

      pasteBuffer = "";
      return undefined;
    }

    // Accumulate paste data
    if (pasteBuffer.length > 0) {
      pasteBuffer += data;
      return undefined;
    }

    // Ctrl+P = prev model
    if (data === "\u0010") {
      void cycleModelBackward();
      return { consume: true };
    }
    // Ctrl+N = next model
    if (data === "\u000e") {
      void cycleModelForward();
      return { consume: true };
    }
    return undefined;
  });

  function isImageData(buf: Buffer): boolean {
    // Check magic bytes
    if (buf.length < 4) return false;
    // PNG: \x89PNG
    if (buf[0] === 0x89 && buf[1] === 0x50 && buf[2] === 0x4e && buf[3] === 0x47) return true;
    // JPEG: \xff\xd8
    if (buf[0] === 0xff && buf[1] === 0xd8) return true;
    // GIF: GIF8
    if (buf[0] === 0x47 && buf[1] === 0x49 && buf[2] === 0x46 && buf[3] === 0x38) return true;
    // WebP: RIFF....WEBP
    if (
      buf[0] === 0x52 &&
      buf[1] === 0x49 &&
      buf[2] === 0x46 &&
      buf[3] === 0x46 &&
      buf.length > 12 &&
      buf[8] === 0x57 &&
      buf[9] === 0x45 &&
      buf[10] === 0x42 &&
      buf[11] === 0x50
    )
      return true;
    return false;
  }

  async function handleImagePaste(buf: Buffer): Promise<void> {
    try {
      const { writeFileSync, mkdirSync, existsSync } = await import("node:fs");
      const { join } = await import("node:path");
      const { tmpdir } = await import("node:os");

      const pikoTmp = join(tmpdir(), "piko-images");
      if (!existsSync(pikoTmp)) mkdirSync(pikoTmp, { recursive: true });

      // Detect format from magic bytes
      let ext = ".png";
      if (buf[0] === 0xff && buf[1] === 0xd8) ext = ".jpg";
      else if (buf[0] === 0x47) ext = ".gif";
      else if (buf[0] === 0x52 && buf[1] === 0x49) ext = ".webp";

      const filename = `paste-${Date.now()}${ext}`;
      const filepath = join(pikoTmp, filename);
      writeFileSync(filepath, buf);

      // Insert @path reference into editor
      const currentText = getEditorComponent().getText();
      editor.setText(`${currentText}@${filepath} `);

      chatView.addMessage("system", `📷 Image pasted: ${filename}`);
      chatView.rebuildChat();
      tui.requestRender();
    } catch {
      chatView.addMessage("system", "Failed to process pasted image");
    }
  }

  terminal.setTitle("piko");

  process.on("SIGINT", () => {
    if (abortController && !abortController.signal.aborted) {
      abortController.abort();
      spinner.stop();
      statusLine.set("progress", theme.fg("error", "Interrupted"));
    } else if (!abortController) {
      process.exit(0);
    }
  });

  if (host.sessionFile) {
    chatView.rebuildFromTranscript(transcript);
    await resumeSession();
  } else {
    chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    updateHeader();
    updateFooter();
    chatView.rebuildChat();
  }

  tui.start();
}
