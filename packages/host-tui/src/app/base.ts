import type { Model } from "@earendil-works/pi-ai";
import {
  Box,
  Editor,
  type EditorComponent,
  type LoaderIndicatorOptions,
  ProcessTerminal,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig, Message } from "piko-engine-protocol";
import type { AuthStorage, ModelRegistry, PikoHost, SettingsManager } from "piko-host-runtime";
import { ChatView } from "../chat-view.js";
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
import { getEditorTheme, getTheme } from "../theme.js";
import { createAutocomplete } from "./autocomplete.js";
import type { RunTuiOptions } from "./types.js";

export class BaseApp {
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

  // ---- Sub-elements ----
  readonly headerBox = new Box(0, 0);
  readonly chatBox = new Box(0, 0);
  readonly widgetSlotAbove = new WidgetSlot();
  readonly widgetSlotBelow = new WidgetSlot();

  // ---- Options ----
  readonly opts: {
    modelRegistry?: ModelRegistry;
    authStorage?: AuthStorage;
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

  constructor(
    initialModel: Model<string>,
    initialProviderConfig: EngineProviderConfig,
    options: RunTuiOptions,
    host: PikoHost,
  ) {
    this.opts = {
      modelRegistry: options.modelRegistry,
      authStorage: options.authStorage,
      settingsManager: options.settingsManager,
      noTools: options.noTools,
    };
    this.currentModel = initialModel;
    this.currentProviderConfig = initialProviderConfig;
    this.currentThinkingLevel = options.settingsManager?.getDefaultThinkingLevel() ?? "off";
    this.host = host;

    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);

    this.extensionHost = new ExtensionHost({
      tui: this.tui,
      theme: getTheme(),
      setEditorText: () => {},
      getEditorText: () => "",
      addChatMessage: () => {},
      requestRender: () => this.tui.requestRender(),
      setFooterFactory: () => {},
      setEditorFactory: () => {},
      setWidgetSlot: () => {},
      setStatusSlot: () => {},
      setWorkingIndicatorConfig: () => {},
    });

    this.chatView = new ChatView(this.chatBox);
    this.widgetSlotAbove.bind(this.tui);
    this.widgetSlotBelow.bind(this.tui);

    this.footerComponent = new FooterComponent({
      model: this.currentModel,
      sessionName: this.sessionName,
      messageCount: 0,
      cwd: host.cwd,
    });

    this.editor = new Editor(this.tui, getEditorTheme());
    this.editor.setAutocompleteProvider(createAutocomplete(this.extensionHost));

    this.statusLine = new StatusLine();
    this.spinner = new Spinner();
    this.spinner.bind(this.tui);

    this.extensionHost.bindRuntime({
      setEditorText: (t: string) => this.editor.setText(t),
      getEditorText: () => this.getEditorComponent().getText(),
      addChatMessage: (r: string, t: string) => this.chatView.addMessage(r, t),
      setFooterFactory: (f: FooterFactory | undefined) => {
        this.customFooterFactory = f;
        this.tui.requestRender();
      },
      setEditorFactory: (f: EditorFactory | undefined) => {
        this.customEditorFactory = f;
        this.tui.requestRender();
      },
      setWidgetSlot: (k: string, c: WidgetContent | undefined, p: WidgetPlacement) => {
        (p === "belowEditor" ? this.widgetSlotBelow : this.widgetSlotAbove).set(k, c);
        this.tui.requestRender();
      },
      setStatusSlot: (k: string, t: string | undefined) => {
        this.statusLine.set(k, t);
        (this as any).updateFooter?.();
        this.tui.requestRender();
      },
      setWorkingIndicatorConfig: (c?: LoaderIndicatorOptions) => {
        this.workingIndicatorConfig = c;
        if (this.spinner.active) this.spinner.setIndicator(c);
      },
    });
  }

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
}

export type AppConstructor<T = BaseApp> = new (...args: any[]) => T;
