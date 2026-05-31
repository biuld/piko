/**
 * Main TUI entry point. Creates UI components, wires them together,
 * and starts the terminal event loop.
 */
import type { Model } from "@earendil-works/pi-ai";
import {
  Box,
  Editor,
  type LoaderIndicatorOptions,
  ProcessTerminal,
  Spacer,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { PikoHost } from "piko-host-runtime";
import { InteractiveApprovalHandler } from "../approval-handler.js";
import { ChatView } from "../chat-view.js";
import { handleSlashCommand } from "../commands/index.js";
import { DynamicBorder } from "../components/dynamic-border.js";
import { FooterComponent } from "../components/footer.js";
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
import { getThemeManager } from "../theme/index.js";
import { getEditorTheme, getTheme, setTheme } from "../theme.js";
import { createAutocomplete } from "./autocomplete.js";
import { buildCommandContext } from "./commands-ctx.js";
import type { TuiContext } from "./context.js";
import { createHeaderBox, createUpdateFooter } from "./header-footer.js";
import { makeHostOptions } from "./host-options.js";
import { isImageData, handleImagePaste } from "./image-paste.js";
import { createModelOps } from "./model-ops.js";
import { createSessionOps } from "./session-ops.js";
import { createSubmitOps } from "./submit.js";
import type { RunTuiOptions } from "./types.js";

export type { RunTuiOptions } from "./types.js";

export async function runTui(
  initialModel: Model<string>,
  initialProviderConfig: EngineProviderConfig,
  options: RunTuiOptions = {},
): Promise<void> {
  const terminal = new ProcessTerminal();
  const tui = new TUI(terminal);

  // ---- Build TuiContext ----
  const ctx: TuiContext = {
    tui,
    // placeholder — set after host creation
    host: null!,
    options: {
      modelRegistry: options.modelRegistry,
      settingsManager: options.settingsManager,
      noTools: options.noTools,
    },
    currentModel: initialModel,
    currentProviderConfig: initialProviderConfig,
    currentThinkingLevel: options.settingsManager?.getDefaultThinkingLevel() ?? "off",
    transcript: [],
    sessionName: undefined,
    running: false,
    abortController: null,
    activeOverlay: null,
    cumulativeInput: 0,
    cumulativeOutput: 0,
    cumulativeCacheRead: 0,
    cumulativeCacheWrite: 0,
    cumulativeCost: 0,
    workingIndicatorConfig: undefined,
    customFooterFactory: undefined,
    customEditorFactory: undefined,
    // callbacks — set below
    updateHeader: () => {},
    updateFooter: () => {},
    syncSessionTranscript: async () => {},
    resumeSession: async () => {},
    submitUserMessage: () => {},
    // components — set below
    chatView: null!,
    footerComponent: null!,
    spinner: null!,
    statusLine: null!,
    extensionHost: null!,
    // model ops — set below
    modelOps: null!,
    // expose runStreamWithUI for commands-ctx submitStream
    runStreamWithUI: () => {},
    createNewSession: async () => {},
    cloneSessionCmd: async () => {},
    forkSessionCmd: async () => {},
  };

  // ---- Extension host ----
  const extensionHost = new ExtensionHost({
    tui, theme: getTheme(),
    setEditorText: () => {}, getEditorText: () => "", addChatMessage: () => {},
    requestRender: () => tui.requestRender(),
    setFooterFactory: () => {}, setEditorFactory: () => {},
    setWidgetSlot: () => {}, setStatusSlot: () => {}, setWorkingIndicatorConfig: () => {},
  });
  ctx.extensionHost = extensionHost;

  if (options.extensions?.length) await extensionHost.loadAll(options.extensions);

  // ---- Host ----
  const host = await PikoHost.create({
    ...makeHostOptions(initialModel, initialProviderConfig, { session: options.session }, options.settingsManager, options),
    approvalHandler: new InteractiveApprovalHandler(tui),
    customTools: extensionHost.tools.length > 0 ? extensionHost.tools : undefined,
  });
  ctx.host = host;

  getThemeManager().load(host.cwd);
  const settingsTheme = options.settingsManager?.getTheme();
  if (settingsTheme) { const s = getThemeManager().switchTo(settingsTheme); if (s) setTheme(getThemeManager().get()); }

  ctx.transcript = await host.loadMessages();
  ctx.sessionName = await host.getSessionName();
  if (options.sessionName && !ctx.sessionName) { await host.setSessionName(options.sessionName); ctx.sessionName = options.sessionName; }

  // ---- Chat ----
  const chatBox = new Box(0, 0);
  const chatView = new ChatView(chatBox);
  ctx.chatView = chatView;

  // ---- Widget slots ----
  const widgetSlotAbove = new WidgetSlot();
  const widgetSlotBelow = new WidgetSlot();
  widgetSlotAbove.bind(tui); widgetSlotBelow.bind(tui);

  // ---- Footer ----
  const footerComponent = new FooterComponent({ model: ctx.currentModel, sessionName: ctx.sessionName, messageCount: ctx.transcript.length, cwd: host.cwd });
  ctx.footerComponent = footerComponent;

  function getFooterComponent() {
    if (ctx.customFooterFactory) return ctx.customFooterFactory(tui, getTheme());
    return footerComponent;
  }

  // ---- Editor ----
  const editor = new Editor(tui, getEditorTheme());
  editor.setAutocompleteProvider(createAutocomplete(extensionHost));
  function getEditorComponent() {
    if (ctx.customEditorFactory) {
      const customEditor = ctx.customEditorFactory(tui, getTheme());
      customEditor.onSubmit = (text: string) => editor.onSubmit?.(text);
      return customEditor;
    }
    return editor;
  }

  // ---- Status + Spinner ----
  const statusLine = new StatusLine(); ctx.statusLine = statusLine;
  const spinner = new Spinner(); spinner.bind(tui); ctx.spinner = spinner;

  // ---- Bind extension host ----
  extensionHost.bindRuntime({
    setEditorText: (text: string) => editor.setText(text),
    getEditorText: () => getEditorComponent().getText(),
    addChatMessage: (role: string, text: string) => chatView.addMessage(role, text),
    setFooterFactory: (f: FooterFactory | undefined) => { ctx.customFooterFactory = f; tui.requestRender(); },
    setEditorFactory: (f: EditorFactory | undefined) => { ctx.customEditorFactory = f; tui.requestRender(); },
    setWidgetSlot: (key: string, content: WidgetContent | undefined, placement: WidgetPlacement) => {
      if (placement === "belowEditor") widgetSlotBelow.set(key, content);
      else widgetSlotAbove.set(key, content);
      tui.requestRender();
    },
    setStatusSlot: (key: string, text: string | undefined) => { statusLine.set(key, text); ctx.updateFooter(); tui.requestRender(); },
    setWorkingIndicatorConfig: (config?: LoaderIndicatorOptions) => { ctx.workingIndicatorConfig = config; if (spinner.active) spinner.setIndicator(config); },
  });

  // ---- Session ops ----
  const sessionOps = createSessionOps(ctx);
  ctx.syncSessionTranscript = sessionOps.syncSessionTranscript;
  ctx.resumeSession = sessionOps.resumeSession;
  ctx.createNewSession = sessionOps.createNewSession;
  ctx.cloneSessionCmd = sessionOps.cloneSessionCmd;
  ctx.forkSessionCmd = sessionOps.forkSessionCmd;

  host.onAfterRebind(async () => { await host.restoreFromSession(); await sessionOps.syncSessionTranscript(); });

  // ---- Header / Footer ----
  const headerBox = createHeaderBox(ctx);
  const updateFooter = createUpdateFooter(ctx);
  ctx.updateFooter = updateFooter;

  // ---- Model ops ----
  const modelOps = createModelOps(ctx);
  (ctx as any).modelOps = modelOps;

  // ---- Submit ops ----
  const submitOps = createSubmitOps(ctx);
  ctx.submitUserMessage = submitOps.submitUserMessage;
  ctx.runStreamWithUI = submitOps.runStreamWithUI;

  // ---- Command context ----
  const cmdCtx = buildCommandContext(ctx, editor, () => getEditorComponent().getText());

  // ---- Editor submit ----
  editor.onSubmit = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    if (ctx.running) {
      host.steer(trimmed);
      chatView.addMessage("system", `Queued for next turn: ${trimmed.slice(0, 80)}${trimmed.length > 80 ? "..." : ""}`);
      chatView.rebuildChat(); tui.requestRender();
      return;
    }
    const extCmd = extensionHost.findCommand(trimmed);
    if (extCmd) {
      extCmd.handler(trimmed.slice(extCmd.value.length).trim(), { theme: getTheme(), setEditorText: (t: string) => editor.setText(t), getEditorText: () => getEditorComponent().getText() } as any);
      return;
    }
    if (trimmed.startsWith("/")) { handleSlashCommand(trimmed, cmdCtx); return; }
    submitOps.submitUserMessage(trimmed);
  };

  // ---- Layout ----
  tui.addChild(headerBox);
  tui.addChild(chatBox);
  tui.addChild(widgetSlotAbove);
  tui.addChild(spinner);
  tui.addChild(statusLine);
  tui.addChild(new Spacer(1));
  tui.addChild(widgetSlotBelow);
  tui.addChild(new DynamicBorder((s: string) => getTheme().fg("borderMuted", s)));
  tui.addChild(editor);
  tui.setFocus(editor);
  tui.addChild(getFooterComponent());

  // ---- Image paste ----
  let pasteBuffer = "";
  tui.addInputListener((data: string) => {
    if (ctx.running) return undefined;
    if (data.includes("\x1b[200~")) { pasteBuffer = data.replace("\x1b[200~", ""); return undefined; }
    if (data.includes("\x1b[201~")) {
      const endIdx = data.indexOf("\x1b[201~");
      pasteBuffer += data.slice(0, endIdx);
      if (pasteBuffer.length > 100 && isImageData(Buffer.from(pasteBuffer, "binary"))) {
        void handleImagePaste(ctx, editor, () => getEditorComponent().getText(), Buffer.from(pasteBuffer, "binary"));
        pasteBuffer = ""; return { consume: true };
      }
      pasteBuffer = ""; return undefined;
    }
    if (pasteBuffer.length > 0) { pasteBuffer += data; return undefined; }
    if (data === "\u0010") { void modelOps.cycleModelBackward(); return { consume: true }; }
    if (data === "\u000e") { void modelOps.cycleModelForward(); return { consume: true }; }
    return undefined;
  });

  // ---- SIGINT ----
  terminal.setTitle("piko");
  process.on("SIGINT", () => {
    if (ctx.abortController && !ctx.abortController.signal.aborted) {
      ctx.abortController.abort(); spinner.stop();
      statusLine.set("progress", getTheme().fg("error", "Interrupted"));
    } else if (!ctx.abortController) { process.exit(0); }
  });

  // ---- Start ----
  if (host.sessionFile) {
    chatView.rebuildFromTranscript(ctx.transcript);
    await sessionOps.resumeSession();
  } else {
    chatView.addMessage("system", "New session  |  Ctrl+D submit  Ctrl+C exit  /help");
    updateFooter();
    chatView.rebuildChat();
  }
  tui.start();
}
