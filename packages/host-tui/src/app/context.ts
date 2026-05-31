import type { Model } from "@earendil-works/pi-ai";
import type {
  LoaderIndicatorOptions,
  TUI,
} from "@earendil-works/pi-tui";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { ModelRegistry, PikoHost, ResolvedModel, SettingsManager } from "piko-host-runtime";
import type { ChatView } from "../chat-view.js";
import type { FooterComponent } from "../components/footer.js";
import type { Spinner } from "../components/spinner.js";
import type { StatusLine } from "../components/status-line.js";
import type {
  EditorFactory,
  ExtensionHost,
  FooterFactory,
} from "../extensions/index.js";

export interface TuiContext {
  // ---- Immutable dependencies ----
  tui: TUI;
  host: PikoHost;
  chatView: ChatView;
  footerComponent: FooterComponent;
  spinner: Spinner;
  statusLine: StatusLine;
  extensionHost: ExtensionHost;
  options: {
    modelRegistry?: ModelRegistry;
    settingsManager?: SettingsManager;
    noTools?: boolean;
  };

  // ---- Mutable state ----
  currentModel: Model<string>;
  currentProviderConfig: EngineProviderConfig;
  currentThinkingLevel: string;
  transcript: import("piko-engine-protocol").Message[];
  sessionName: string | undefined;
  running: boolean;
  abortController: AbortController | null;
  activeOverlay: { hide(): void } | null;
  cumulativeInput: number;
  cumulativeOutput: number;
  cumulativeCacheRead: number;
  cumulativeCacheWrite: number;
  cumulativeCost: number;
  workingIndicatorConfig: LoaderIndicatorOptions | undefined;

  // ---- Mutable factories ----
  customFooterFactory: FooterFactory | undefined;
  customEditorFactory: EditorFactory | undefined;

  // ---- Callbacks (set by index.ts) ----
  updateHeader: () => void;
  updateFooter: () => void;
  syncSessionTranscript: (msg?: string) => Promise<void>;
  resumeSession: () => Promise<void>;
  submitUserMessage: (text: string) => void;
  runStreamWithUI: (stream: ReturnType<PikoHost["streamPrompt"]>, displayText: string) => void;
  createNewSession: () => Promise<void>;
  cloneSessionCmd: () => Promise<void>;
  forkSessionCmd: (entryId: string, setEditorText: (t: string) => void) => Promise<void>;

  // ---- Model ops (set by index.ts) ----
  modelOps: {
    getModelList(): Array<{ model: Model<string>; providerConfig: EngineProviderConfig }>;
    getModelIds(): string[];
    resolveModel(id: string, prov: string): ResolvedModel | null;
    applyModelChange(found: ResolvedModel): void;
    cycleModelForward(): Promise<void>;
    cycleModelBackward(): Promise<void>;
  };
}
