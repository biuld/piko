import type { Model } from "@earendil-works/pi-ai";
import {
  Box,
  Editor,
  type EditorComponent,
  type LoaderIndicatorOptions,
  ProcessTerminal,
  Spacer,
  Text,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig, Message } from "piko-engine-protocol";
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
import { ChatView } from "../chat-view.js";
import { handleSlashCommand } from "../commands/index.js";
import { DynamicBorder } from "../components/dynamic-border.js";
import { FooterComponent } from "../components/footer.js";
import { getContextHints } from "../components/key-hints.js";
import { Spinner } from "../components/spinner.js";
import { StatusLine } from "../components/status-line.js";
import { WidgetSlot } from "../components/widget-slot.js";
import {
  type EditorFactory,
  ExtensionHost,
  type FooterFactory,
  type WidgetContent,
  type WidgetPlacement,
} from "../extensions/index.js";
import { formatSessionTreeLines } from "../session-tree.js";
import { getThemeManager } from "../theme/index.js";
import { getEditorTheme, getTheme, setTheme } from "../theme.js";
import { createAutocomplete } from "./autocomplete.js";
import { buildCommandContext } from "./commands-ctx.js";
import { isImageData, handleImagePaste } from "./image-paste.js";
import type { RunTuiOptions } from "./types.js";

export class App {
  // ---- Components ----
  readonly tui: TUI;
  readonly terminal: ProcessTerminal;
  readonly host: PikoHost;
  readonly chatView: ChatView;
  readonly footerComponent: FooterComponent;
  readonly spinner: Spinner;
  readonly statusLine: StatusLine;
  readonly extensionHost: ExtensionHost;
  readonly editor: Editor;

  // ---- Options (immutable after construction) ----
  readonly opts: {
    modelRegistry?: ModelRegistry;
    settingsManager?: SettingsManager;
    noTools?: boolean;
  };

  // ---- Mutable state ----
  currentModel: Model<string>;
  currentProviderConfig: EngineProviderConfig;
  currentThinkingLevel: string;
  transcript: Message[] = [];
  sessionName: string | undefined;
  running = false;
  abortController: AbortController | null = null;
  activeOverlay: { hide(): void } | null = null;
  cumulativeInput = 0;
  cumulativeOutput = 0;
  cumulativeCacheRead = 0;
  cumulativeCacheWrite = 0;
  cumulativeCost = 0;
  workingIndicatorConfig: LoaderIndicatorOptions | undefined;
  customFooterFactory: FooterFactory | undefined;
  customEditorFactory: EditorFactory | undefined;

  // ---- Private UI elements ----
  private headerBox = new Box(0, 0);
  private widgetSlotAbove = new WidgetSlot();
  private widgetSlotBelow = new WidgetSlot();

  // ---- ChatBox (owned by ChatView, re-referenced here for layout) ----
  private chatBox = new Box(0, 0);

  constructor(
    initialModel: Model<string>,
    initialProviderConfig: EngineProviderConfig,
    options: RunTuiOptions,
    host: PikoHost,
  ) {
    this.opts = { modelRegistry: options.modelRegistry, settingsManager: options.settingsManager, noTools: options.noTools };
    this.currentModel = initialModel;
    this.currentProviderConfig = initialProviderConfig;
    this.currentThinkingLevel = options.settingsManager?.getDefaultThinkingLevel() ?? "off";
    this.host = host;

    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);

    this.extensionHost = new ExtensionHost({
      tui: this.tui, theme: getTheme(),
      setEditorText: () => {}, getEditorText: () => "", addChatMessage: () => {},
      requestRender: () => this.tui.requestRender(),
      setFooterFactory: () => {}, setEditorFactory: () => {},
      setWidgetSlot: () => {}, setStatusSlot: () => {}, setWorkingIndicatorConfig: () => {},
    });

    this.chatView = new ChatView(this.chatBox);

    this.widgetSlotAbove.bind(this.tui);
    this.widgetSlotBelow.bind(this.tui);

    this.footerComponent = new FooterComponent({ model: this.currentModel, sessionName: this.sessionName, messageCount: 0, cwd: host.cwd });

    this.editor = new Editor(this.tui, getEditorTheme());
    this.editor.setAutocompleteProvider(createAutocomplete(this.extensionHost));

    this.statusLine = new StatusLine();
    this.spinner = new Spinner();
    this.spinner.bind(this.tui);

    // Bind extension host runtime
    this.extensionHost.bindRuntime({
      setEditorText: (t: string) => this.editor.setText(t),
      getEditorText: () => this.getEditorComponent().getText(),
      addChatMessage: (r: string, t: string) => this.chatView.addMessage(r, t),
      setFooterFactory: (f: FooterFactory | undefined) => { this.customFooterFactory = f; this.tui.requestRender(); },
      setEditorFactory: (f: EditorFactory | undefined) => { this.customEditorFactory = f; this.tui.requestRender(); },
      setWidgetSlot: (k: string, c: WidgetContent | undefined, p: WidgetPlacement) => {
        (p === "belowEditor" ? this.widgetSlotBelow : this.widgetSlotAbove).set(k, c);
        this.tui.requestRender();
      },
      setStatusSlot: (k: string, t: string | undefined) => { this.statusLine.set(k, t); this.updateFooter(); this.tui.requestRender(); },
      setWorkingIndicatorConfig: (c?: LoaderIndicatorOptions) => { this.workingIndicatorConfig = c; if (this.spinner.active) this.spinner.setIndicator(c); },
    });
  }

  // ---- Editor helpers ----

  getEditorComponent(): EditorComponent {
    if (this.customEditorFactory) {
      const ce = this.customEditorFactory(this.tui, getTheme());
      ce.onSubmit = (t: string) => this.editor.onSubmit?.(t);
      return ce;
    }
    return this.editor;
  }

  getFooterComponent() {
    return this.customFooterFactory?.call(null, this.tui, getTheme()) ?? this.footerComponent;
  }

  // =========================================================================
  // Header / Footer
  // =========================================================================

  updateHeader(): void {
    this.headerBox.clear();
    const t = getTheme();
    this.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
    const name = this.sessionName ?? this.host.sessionId.slice(-8);
    this.headerBox.addChild(new Text(t.fg("accent", ` piko  ${this.currentModel.provider}/${this.currentModel.id}  session ${name}  ${this.transcript.length} msgs `), 1, 0));
    this.headerBox.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
  }

  updateFooter(): void {
    const statuses = [...this.statusLine.getEntries()];
    const hints = this.running ? getContextHints("streaming") : this.activeOverlay ? getContextHints("overlay") : getContextHints("normal");
    this.footerComponent.update({
      model: this.currentModel, sessionName: this.sessionName, messageCount: this.transcript.length, cwd: this.host.cwd,
      totalInputTokens: this.cumulativeInput || undefined,
      totalOutputTokens: this.cumulativeOutput || undefined,
      totalCacheRead: this.cumulativeCacheRead || undefined,
      totalCacheWrite: this.cumulativeCacheWrite || undefined,
      totalCost: this.cumulativeCost || undefined,
      contextWindow: (this.currentModel as { contextWindow?: number }).contextWindow,
      contextPercent: (this.currentModel as { contextWindow?: number }).contextWindow
        ? getContextPercent(this.cumulativeInput, (this.currentModel as { contextWindow?: number }).contextWindow!) : undefined,
      extensionStatuses: statuses.length > 0 ? statuses : undefined,
      keyHints: hints || undefined,
    });
  }

  // =========================================================================
  // Session
  // =========================================================================

  async syncTranscript(msg?: string): Promise<void> {
    const loaded = await this.host.loadMessages();
    this.sessionName = await this.host.getSessionName();
    this.transcript = [...loaded];
    this.updateHeader(); this.updateFooter();
    this.chatView.rebuildFromTranscript(this.transcript, msg);
    this.chatView.rebuildChat(); this.tui.requestRender();
  }

  async resume(): Promise<void> {
    const loaded = await this.host.loadMessages();
    if (loaded.length === 0) { this.chatView.addMessage("system", `Session ${this.host.sessionId} not found or empty`); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
    await this.host.restoreFromSession();
    this.currentModel = this.host.getConfig().model;
    this.currentProviderConfig = this.host.getConfig().provider;
    this.currentThinkingLevel = this.host.getThinkingLevel();
    this.chatView.addMessage("system", `Resumed session ${this.host.sessionId} (${loaded.length} messages)`);
    this.chatView.rebuildChat(); this.tui.requestRender();
  }

  async newSession(): Promise<void> {
    await this.host.newSession();
    this.chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    this.chatView.rebuildChat(); this.tui.requestRender();
  }

  async clone(): Promise<void> {
    if (!this.host.isSessionPersisted()) { this.chatView.addMessage("system", "Clone requires a saved session"); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
    await this.host.cloneSession();
    this.chatView.addMessage("system", `Cloned branch into session ${this.host.sessionId}`);
    this.chatView.rebuildChat(); this.tui.requestRender();
  }

  async fork(entryId: string): Promise<void> {
    if (!this.host.isSessionPersisted()) { this.chatView.addMessage("system", "Fork requires a saved session"); this.chatView.rebuildChat(); this.tui.requestRender(); return; }
    const result = await this.host.forkSession(entryId);
    const suffix = result.selectedText ? `\nOriginal prompt: ${result.selectedText}` : "";
    this.chatView.addMessage("system", `Forked into session ${this.host.sessionId}${suffix}`);
    this.chatView.rebuildChat(); this.tui.requestRender();
    if (result.selectedText) this.editor.setText(result.selectedText);
  }

  // =========================================================================
  // Model
  // =========================================================================

  getModelList(): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }> {
    if (this.opts.modelRegistry) {
      return this.opts.modelRegistry.listScopedModels().map((m) => ({
        model: m, providerConfig: this.opts.modelRegistry!.resolve(m.id, m.provider)?.providerConfig ?? this.currentProviderConfig,
      }));
    }
    return listAvailableModels().flatMap((p) => p.models.map((m) => {
      const found = findModel(m.id, p.provider);
      return { model: { provider: p.provider, id: m.id, name: m.name } as Model<string>, providerConfig: found?.providerConfig ?? this.currentProviderConfig };
    }));
  }

  getModelIds(): string[] {
    if (this.opts.modelRegistry) return this.opts.modelRegistry.listScopedModels().map((m) => `${m.provider}/${m.id}`);
    return listAvailableModels().flatMap((p) => p.models.map((m) => `${p.provider}/${m.id}`));
  }

  resolveModel(id: string, prov: string): ResolvedModel | null {
    if (this.opts.modelRegistry) return this.opts.modelRegistry.resolve(id, prov);
    const f = findModel(id, prov);
    return f ? { model: f.model, providerConfig: f.providerConfig } : null;
  }

  applyModelChange(found: ResolvedModel): void {
    this.currentModel = found.model;
    this.currentProviderConfig = found.providerConfig;
    this.host.setConfig(createHostConfig(found.model, found.providerConfig, createDefaultSettings({
      maxSteps: 10, parallelTools: false, allowToolCalls: !this.opts.noTools, allowApprovals: true,
    })));
    this.host.setThinkingLevel(this.currentThinkingLevel);
  }

  async cycleModel(forward: boolean): Promise<void> {
    const ids = this.getModelIds();
    const currentId = `${this.currentModel.provider}/${this.currentModel.id}`;
    const idx = ids.indexOf(currentId);
    if (idx === -1 || ids.length === 0) return;
    const nextId = ids[(idx + (forward ? 1 : -1) + ids.length) % ids.length];
    const [prov, id] = nextId.split("/");
    const found = this.resolveModel(id, prov);
    if (!found) return;
    this.applyModelChange(found);
    const label = `${found.model.provider}/${found.model.id}`;
    this.chatView.addMessage("system", `Switched to ${label}`);
    this.updateHeader(); this.updateFooter();
    this.chatView.rebuildChat(); this.tui.requestRender();
  }

  // =========================================================================
  // Submit & Streaming
  // =========================================================================

  private runStream(stream: ReturnType<typeof this.host.streamPrompt>): void {
    let hasAssistant = false;
    const tcIds = new Map<string, string>();
    const tcNames = new Map<string, string>();
    void (async () => {
      for await (const e of stream) {
        if (e.type === "message_delta") { (hasAssistant ? this.chatView.updateLastAssistant : (hasAssistant = true, this.chatView.addMessage))("assistant", (e as any).delta); this.chatView.rebuildChat(); this.tui.requestRender(); }
        else if (e.type === "thinking_delta") { this.statusLine.set("progress", getTheme().fg("muted", "Thinking...")); this.tui.requestRender(); }
        else if (e.type === "tool_call_start") {
          this.statusLine.set("progress", getTheme().fg("toolPendingBg", `Running ${e.name}...`));
          tcIds.set(e.id, this.chatView.startToolCall(e.name, e.args, this.host.cwd));
          tcNames.set(e.id, e.name);
          this.chatView.rebuildChat(); this.tui.requestRender();
          this.extensionHost.dispatchEvent({ type: "tool_call_start", name: e.name, args: e.args as Record<string, unknown> });
        } else if (e.type === "tool_call_end") {
          const n = tcNames.get(e.id) ?? "tool";
          this.statusLine.set("progress", getTheme().fg(e.isError ? "error" : "success", `${n} ${e.isError ? "failed" : "completed"}`));
          const tid = tcIds.get(e.id); if (tid) this.chatView.endToolCall(tid, e.result, e.isError);
          this.chatView.rebuildChat(); this.tui.requestRender();
          this.extensionHost.dispatchEvent({ type: "tool_call_end", name: n, result: e.result, isError: e.isError });
        }
      }
      const r = await stream.result();
      this.spinner.stop(); this.abortController = null;
      this.transcript = r.messages;
      const u = computeCumulativeUsage(r.messages);
      this.cumulativeInput += u.input; this.cumulativeOutput += u.output;
      this.cumulativeCacheRead += u.cacheRead; this.cumulativeCacheWrite += u.cacheWrite; this.cumulativeCost += u.cost;
      this.chatView.rebuildFromTranscript(this.transcript,
        r.status === "max_steps" ? "Stopped after reaching max steps" : r.status === "aborted" ? "Interrupted" : r.status === "error" ? "Run failed" : undefined);
      this.updateHeader(); this.updateFooter(); this.statusLine.set("progress", undefined);
      this.running = false;
      this.chatView.rebuildChat(); this.tui.requestRender();
      this.extensionHost.dispatchEvent({ type: "turn_end", status: r.status, steps: this.transcript.length });
    })().catch((err: unknown) => {
      this.spinner.stop(); this.abortController = null; this.running = false;
      this.chatView.addMessage("system", err instanceof Error ? err.message : String(err));
      this.chatView.rebuildChat(); this.tui.requestRender();
    });
  }

  submit(text: string): void {
    const t = text.trim(); if (!t) return;
    const { expanded } = processFileArguments(t, this.host.cwd);
    this.running = true; this.abortController = new AbortController();
    this.spinner.start(); if (this.workingIndicatorConfig) this.spinner.setIndicator(this.workingIndicatorConfig);
    this.statusLine.set("progress", "");
    this.chatView.addMessage("user", expanded); this.chatView.rebuildChat(); this.tui.requestRender();
    this.extensionHost.dispatchEvent({ type: "message", role: "user", content: expanded });
    this.runStream(this.host.streamPrompt(expanded, {}, this.abortController.signal));
  }

  submitStream(factory: (sig: AbortSignal) => ReturnType<typeof this.host.streamPrompt>, label: string): void {
    this.editor.setText("");
    this.running = true; this.abortController = new AbortController();
    const stream = factory(this.abortController.signal);
    this.spinner.start(); if (this.workingIndicatorConfig) this.spinner.setIndicator(this.workingIndicatorConfig);
    this.statusLine.set("progress", "");
    this.chatView.addMessage("user", label); this.chatView.rebuildChat(); this.tui.requestRender();
    this.extensionHost.dispatchEvent({ type: "message", role: "user", content: label });
    this.runStream(stream);
  }

  // =========================================================================
  // Init — wiring, layout, start
  // =========================================================================

  async init(options: RunTuiOptions): Promise<void> {
    // Extensions
    if (options.extensions?.length) await this.extensionHost.loadAll(options.extensions);

    // Themes
    getThemeManager().load(this.host.cwd);
    const st = options.settingsManager?.getTheme();
    if (st) { const s = getThemeManager().switchTo(st); if (s) setTheme(getThemeManager().get()); }

    // Transcript
    this.transcript = await this.host.loadMessages();
    this.sessionName = await this.host.getSessionName();
    if (options.sessionName && !this.sessionName) { await this.host.setSessionName(options.sessionName); this.sessionName = options.sessionName; }

    this.host.onAfterRebind(async () => { await this.host.restoreFromSession(); await this.syncTranscript(); });

    const cmdCtx = buildCommandContext(this);

    // Editor submit
    this.editor.onSubmit = (text: string) => {
      const t = text.trim(); if (!t) return;
      if (this.running) {
        this.host.steer(t);
        this.chatView.addMessage("system", `Queued for next turn: ${t.slice(0, 80)}${t.length > 80 ? "..." : ""}`);
        this.chatView.rebuildChat(); this.tui.requestRender(); return;
      }
      const extCmd = this.extensionHost.findCommand(t);
      if (extCmd) { extCmd.handler(t.slice(extCmd.value.length).trim(), { theme: getTheme(), setEditorText: (s: string) => this.editor.setText(s), getEditorText: () => this.getEditorComponent().getText() } as any); return; }
      if (t.startsWith("/")) { handleSlashCommand(t, cmdCtx); return; }
      this.submit(t);
    };

    // Layout
    const tui = this.tui;
    tui.addChild(this.headerBox);
    tui.addChild(this.chatBox);
    tui.addChild(this.widgetSlotAbove);
    tui.addChild(this.spinner);
    tui.addChild(this.statusLine);
    tui.addChild(new Spacer(1));
    tui.addChild(this.widgetSlotBelow);
    tui.addChild(new DynamicBorder((s: string) => getTheme().fg("borderMuted", s)));
    tui.addChild(this.editor);
    tui.setFocus(this.editor);
    tui.addChild(this.getFooterComponent());

    // Image paste
    let pb = "";
    tui.addInputListener((data: string) => {
      if (this.running) return undefined;
      if (data.includes("\x1b[200~")) { pb = data.replace("\x1b[200~", ""); return undefined; }
      if (data.includes("\x1b[201~")) {
        const ei = data.indexOf("\x1b[201~"); pb += data.slice(0, ei);
        if (pb.length > 100 && isImageData(Buffer.from(pb, "binary"))) { void this._imagePaste(Buffer.from(pb, "binary")); pb = ""; return { consume: true }; }
        pb = ""; return undefined;
      }
      if (pb.length > 0) { pb += data; return undefined; }
      if (data === "\u0010") { void this.cycleModel(false); return { consume: true }; }
      if (data === "\u000e") { void this.cycleModel(true); return { consume: true }; }
      return undefined;
    });

    // SIGINT
    this.terminal.setTitle("piko");
    process.on("SIGINT", () => {
      if (this.abortController && !this.abortController.signal.aborted) { this.abortController.abort(); this.spinner.stop(); this.statusLine.set("progress", getTheme().fg("error", "Interrupted")); }
      else if (!this.abortController) process.exit(0);
    });

    // Start
    this.updateHeader();
    if (this.host.sessionFile) { this.chatView.rebuildFromTranscript(this.transcript); await this.resume(); }
    else { this.chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help"); this.updateFooter(); this.chatView.rebuildChat(); }
    tui.start();
  }

  private async _imagePaste(buf: Buffer): Promise<void> {
    await handleImagePaste(this, this.editor, () => this.getEditorComponent().getText(), buf);
  }
}
