// ============================================================================
// TUI State Model
//
// Domain state: facts from Host/Engine (transcript, model, session, usage)
// View state:  how the user sees these facts (selected msg, expanded tool, etc.)
// Layout state: how view state maps to terminal dimensions
//
// Domain + View + viewport → layout policies → Layout state + render view models
//
// New UX runtime subsystems: notifications, surfaces, timeline, focus
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { TuiFocusState } from "../focus/types.js";
import type { TuiNotification } from "../notifications/types.js";
import type { TuiSurfaceState } from "../surfaces/types.js";
import type { TuiTimelineState } from "../timeline/types.js";

// ============================================================================
// Domain state
// ============================================================================

export interface TuiSessionState {
  /** Session ID (undefined for new unsaved sessions) */
  sessionId?: string;
  /** Human-readable session name */
  sessionName?: string;
  /** Current working directory */
  cwd: string;
  /** Number of messages in transcript */
  messageCount: number;
  /** Git branch in cwd, if any */
  gitBranch?: string;
}

export interface TuiModelState {
  /** Currently selected model */
  current: Model<string>;
  /** Provider configuration */
  providerConfig: EngineProviderConfig;
  /** Current thinking level */
  thinkingLevel: string;
  /** All available models (for selectors) */
  availableModels: Model<string>[];
}

export interface TuiUsageState {
  /** Cumulative input tokens */
  inputTokens: number;
  /** Cumulative output tokens */
  outputTokens: number;
  /** Cumulative cache read tokens */
  cacheReadTokens: number;
  /** Cumulative cache write tokens */
  cacheWriteTokens: number;
  /** Cumulative cost in USD */
  totalCost: number;
  /** Context window size (tokens) */
  contextWindow?: number;
  /** Context usage as percentage (0-100+) */
  contextPercent?: number;
}

export interface ToolBlockViewModel {
  /** Unique tool call identifier */
  toolCallId: string;
  /** Tool name */
  name: string;
  /** Tool arguments */
  args: unknown;
  /** Execution status */
  status: "pending" | "running" | "success" | "error";
  /** Execution result (when complete) */
  result?: unknown;
  /** Whether the tool block is collapsed */
  isCollapsed: boolean;
}

export interface TuiMessageViewModel {
  /** Unique message id within transcript */
  id: string;
  /** Message role */
  role: "user" | "assistant" | "tool" | "branchSummary" | "compactionSummary";
  /** Display text */
  text: string;
  /** Specialized rendering kind */
  kind?: "skill" | "template";
  /** Tool block details (for tool messages) */
  toolBlock?: ToolBlockViewModel;
  /** Whether this message is still streaming */
  isStreaming?: boolean;
}

export interface TuiStreamState {
  /** Current stream status */
  status: "idle" | "running" | "aborting";
  /** Accumulated assistant text so far */
  assistantText: string;
  /** Whether thinking is active */
  thinkingActive: boolean;
  /** Currently executing tool call id */
  currentToolCallId?: string;
  /** Current tool name being executed */
  currentToolName?: string;
  /** Queue info from lifecycle events */
  queueInfo?: string;
  /** Abort controller for the current stream (not serialized) */
  abortController?: AbortController;
}

// ============================================================================
// View state
// ============================================================================

export type TuiOverlayKind =
  | "model"
  | "thinking"
  | "settings"
  | "resume"
  | "fork"
  | "login"
  | "oauth"
  | "rename"
  | "tree"
  | "approval";

export interface TuiOverlayState {
  kind: TuiOverlayKind;
  isOpen: boolean;
  placement: "modal" | "drawer";
  /** Overlay-specific state if needed */
  data?: unknown;
}

export interface TuiInputState {
  /** Current editor text */
  text: string;
  /** Whether the input has focus */
  focused: boolean;
}

// ============================================================================
// Layout state
// ============================================================================

export type LayoutMode = "regular" | "compact" | "minimal";
export type LayoutActiveRegion = "chat" | "editor" | "overlay";
export type BottomBarDensity = "full" | "compact" | "minimal";
export type BottomBarField =
  | "model"
  | "session"
  | "branch"
  | "tokens"
  | "cost"
  | "cwd"
  | "mode"
  | "hints";

export interface TuiLayoutState {
  /** Terminal dimensions */
  viewport: { width: number; height: number };
  /** Layout mode based on viewport size */
  mode: LayoutMode;
  /** Currently focused region */
  activeRegion: LayoutActiveRegion;
  /** Bottom bar configuration */
  bottomBar: {
    density: BottomBarDensity;
    visibleFields: BottomBarField[];
  };
  /** Chat area configuration */
  chat: {
    selectedMessageId?: string;
    collapsedToolCallIds: Set<string>;
  };
  /** Overlay layout (derived from overlay state + viewport) */
  overlay?: {
    kind: TuiOverlayKind;
    placement: "modal" | "drawer";
  };
}

// ============================================================================
// Extension slots
// ============================================================================

export interface TuiExtensionSlots {
  /** Status text from extensions, keyed by slot name */
  statusSlots: Map<string, string>;
  /** Widget content above editor */
  widgetAbove?: unknown;
  /** Widget content below editor */
  widgetBelow?: unknown;
  /** Custom footer factory key */
  footerFactory?: string;
  /** Custom editor factory key */
  editorFactory?: string;
  /** Working/loading indicator config */
  workingIndicator?: unknown;
}

// ============================================================================
// Root state
// ============================================================================

export interface TuiState {
  /** Domain facts from Host/Engine */
  session: TuiSessionState;
  model: TuiModelState;
  transcript: TuiMessageViewModel[];
  usage: TuiUsageState;

  /** Streaming state */
  stream: TuiStreamState;

  /** View state */
  input: TuiInputState;
  overlay: TuiOverlayState | null;

  /** Layout state (derived from domain/view + viewport) */
  layout: TuiLayoutState;

  /** Extension slot state */
  extensions: TuiExtensionSlots;

  /** Whether the app is currently running (not yet shut down) */
  running: boolean;

  /** Autocomplete state (active + selected index) */
  autocomplete?: {
    active: boolean;
    selectedIndex: number;
    acceptToken: number;
  };

  // ---- UX Runtime subsystems ----
  /** In-memory notification history for current session */
  notifications: TuiNotification[];
  /** Active surface stack */
  surfaces: TuiSurfaceState[];
  /** Focus ownership state */
  focus: TuiFocusState;
  /** Timeline view state (scroll, expansion, streaming) */
  timeline: TuiTimelineState;
}

// ============================================================================
// Default state factory
// ============================================================================

import { createDefaultTimelineState } from "../timeline/types.js";

export function createDefaultTuiState(
  model: Model<string>,
  providerConfig: EngineProviderConfig,
  cwd: string,
  thinkingLevel?: string,
): TuiState {
  return {
    session: {
      cwd,
      messageCount: 0,
    },
    model: {
      current: model,
      providerConfig,
      thinkingLevel: thinkingLevel ?? "off",
      availableModels: [],
    },
    transcript: [],
    usage: {
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      totalCost: 0,
    },
    stream: {
      status: "idle",
      assistantText: "",
      thinkingActive: false,
    },
    input: {
      text: "",
      focused: true,
    },
    overlay: null,
    layout: {
      viewport: { width: 80, height: 24 },
      mode: "regular",
      activeRegion: "editor",
      bottomBar: {
        density: "full",
        visibleFields: ["model", "session", "branch", "tokens", "cost", "cwd", "mode", "hints"],
      },
      chat: {
        collapsedToolCallIds: new Set(),
      },
    },
    extensions: {
      statusSlots: new Map(),
    },
    running: true,

    // UX Runtime subsystems
    notifications: [],
    surfaces: [],
    focus: {
      activeOwnerId: "editor",
      stack: ["editor"],
      region: "editor",
      path: ["editor"],
    },
    timeline: createDefaultTimelineState(),
  };
}
