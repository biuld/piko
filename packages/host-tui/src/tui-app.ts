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
import { COMMANDS, type CommandContext, handleSlashCommand } from "./commands.js";
import { DynamicBorder } from "./components/dynamic-border.js";
import { FooterComponent } from "./components/footer.js";
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
  openModelSelector,
  openResumeSelector,
  openSettingsSelector,
  openThinkingSelector,
  openTreeSelector,
} from "./overlays/index.js";
import { formatSessionTreeLines } from "./session-tree.js";
import { runStreaming } from "./streaming.js";
import { getContextHints } from "./components/key-hints.js";
import { getEditorTheme, getTheme, setTheme } from "./theme.js";
import { getThemeManager } from "./theme/manager.js";

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

  // Mutable model state (for cycling)
  let currentModel = initialModel;
  let currentProviderConfig = initialProviderConfig;

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
    ...makeHostOptions(currentModel, currentProviderConfig, { session: options.session }, options.settingsManager, options),
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
  let thinkingLevel = "off";

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
  // Use ModelRegistry when available (for scoped models + auth), fall back to direct discovery
  function getModelList(): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
    if (options.modelRegistry) {
      const registryModels = options.modelRegistry.listScopedModels();
      return registryModels.map((m) => ({
        model: m,
        providerConfig: currentProviderConfig,
      }));
    }
    const allProviders = listAvailableModels();
    return allProviders.flatMap((p) =>
      p.models.map((m) => ({
        model: { provider: p.provider, id: m.id, name: m.name } as Model<string>,
        providerConfig: currentProviderConfig,
      })),
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

  const availableModels = getModelList();
  const allModelIds = getModelIds();

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
      currentModel = found.model;
      currentProviderConfig = found.providerConfig;
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
      currentModel = found.model;
      currentProviderConfig = found.providerConfig;
      chatView.addMessage("system", `Switched to ${found.model.provider}/${found.model.id}`);
      updateHeader();
      updateFooter();
      chatView.rebuildChat();
      tui.requestRender();
    }
  }

  const cmdCtx: CommandContext = {
    host,
    model: { provider: currentModel.provider, id: currentModel.id, name: currentModel.name },
    sessionName,
    setSessionName: (name: string | undefined) => {
      sessionName = name;
    },
    transcriptLength: transcript.length,
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
      await openModelSelector(overlayCtx, availableModels);
    },
    cycleModelForward,
    cycleModelBackward,
    thinkingLevel,
    setThinkingLevel: (level: string) => {
      thinkingLevel = level;
      chatView.addMessage("system", `Thinking level: ${level}`);
      chatView.rebuildChat();
      tui.requestRender();
    },
    doThinkingSelector: async () => {
      const level = await openThinkingSelector(overlayCtx, thinkingLevel);
      if (level) {
        thinkingLevel = level;
        chatView.addMessage("system", `Thinking level: ${level}`);
        chatView.rebuildChat();
        tui.requestRender();
      }
    },
    doLoginSelector: async (provider: string) => {
      await openLoginDialog(overlayCtx, provider);
    },
    doSettingsSelector: async () => {
      let sm = options.settingsManager;
      if (!sm) {
        const { SettingsManager: SM } = await import("piko-host-runtime");
        sm = SM.create(host.cwd);
      }
      await openSettingsSelector(overlayCtx, sm);
    },
    switchTheme: (name: string) => {
      const manager = getThemeManager();
      const ok = manager.switchTo(name);
      if (ok) setTheme(manager.get());
      return ok;
    },
    currentTheme: getThemeManager().getCurrentName(),
    listModels: listAvailableModels,
    formatSessions: formatSessionTreeLines,
  };

  // ---- Editor submit handler ----
  editor.onSubmit = (text: string) => {
    if (running) return;
    const trimmed = text.trim();
    if (!trimmed) return;

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

    let hasAssistant = false;
    const streamToolIds: Map<string, string> = new Map();
    void runStreaming(
      host,
      expandedText,
      abortController.signal,
      {
        onAssistantDelta: (partial) => {
          if (!hasAssistant) {
            chatView.addMessage("assistant", partial);
            hasAssistant = true;
          } else {
            chatView.updateLastAssistant(partial);
          }
          chatView.rebuildChat();
          tui.requestRender();
        },
        onThinkingDelta: () => {
          statusLine.set("progress", theme.fg("muted", "Thinking..."));
          tui.requestRender();
        },
        onToolCallStart: (name, args, eventId) => {
          statusLine.set("progress", theme.fg("toolPendingBg", `Running ${name}...`));
          const tid = chatView.startToolCall(name, args, host.cwd);
          streamToolIds.set(eventId, tid);
          chatView.rebuildChat();
          tui.requestRender();
          extensionHost.dispatchEvent({ type: "tool_call_start", name, args: args as Record<string, unknown> });
        },
        onToolCallEnd: (name, result, isError, eventId) => {
          statusLine.set(
            "progress",
            isError
              ? theme.fg("error", `${name} failed`)
              : theme.fg("success", `${name} completed`),
          );
          const tid = streamToolIds.get(eventId);
          if (tid) chatView.endToolCall(tid, result, isError);
          chatView.rebuildChat();
          tui.requestRender();
          extensionHost.dispatchEvent({ type: "tool_call_end", name, result, isError });
        },
      },
      thinkingLevel,
    )
      .then((result) => {
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
      })
      .catch((error: unknown) => {
        spinner.stop();
        abortController = null;
        running = false;
        const message = error instanceof Error ? error.message : String(error);
        chatView.addMessage("system", message);
        chatView.rebuildChat();
        tui.requestRender();
      });
  };

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

  // ---- Global keyboard shortcuts ----
  tui.addInputListener((data: string) => {
    if (running) return undefined;
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
