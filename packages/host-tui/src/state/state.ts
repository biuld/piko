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

import type { TuiFocusState } from "../focus/types.js";
import type { TuiNotification } from "../notifications/types.js";
import type { StatusQueueContract } from "../renderer/opentui/status/types.js";
import type {
  Model,
  ModelProviderConfig,
  RuntimeAssistantContentBlock,
  RuntimeMessage,
} from "../shared/index.js";
import type { SurfaceState } from "../surfaces/types.js";
import type { TimelineProjection } from "../timeline/projection.js";
import { createProjection } from "../timeline/projection.js";
import type { TuiTimelineState } from "../timeline/types.js";
import { createDefaultTimelineState } from "../timeline/types.js";

// ============================================================================
// Domain state
// ============================================================================

export interface TreeNavigationState {
  status: "idle" | "running" | "failed";
  operationId?: string;
  entryId?: string;
  error?: string;
}

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
  /** Explicit navigation state */
  navigation: TreeNavigationState;
}

export interface TuiModelState {
  /** Currently selected model */
  current: Model<string>;
  /** Provider configuration */
  providerConfig: ModelProviderConfig;
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
  /** Stable internal identity; provider toolCallId may repeat across runs. */
  toolEntityId?: string;
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
  /** Duration of tool execution in milliseconds */
  duration?: number;
  /** Exit code for bash/exec tools */
  exitCode?: number;
}

export interface TuiMessageViewModel {
  /** Unique message id within transcript */
  id: string;
  /** Message role */
  role: "user" | "assistant" | "tool" | "branchSummary" | "compactionSummary" | "custom";
  /** Display text */
  text: string;
  /** Specialized rendering kind */
  kind?: "skill" | "template";
  /** Preserved customType from custom_message entries */
  customType?: string;
  /** Tool block details (for tool messages) */
  toolBlock?: ToolBlockViewModel;
  /** Whether this message is still streaming */
  isStreaming?: boolean;
  /** Thinking text for assistant messages */
  thinkingText?: string;
  /** Whether this message represents an error state */
  isError?: boolean;
  /** Error message to display */
  errorMessage?: string;
  /** Token count before compaction (for compaction summaries) */
  tokensBefore?: number;
  /** Structured RuntimeMessage payload for block-based rendering */
  message?: RuntimeMessage;
  /** Ordered assistant content blocks */
  content?: RuntimeAssistantContentBlock[];
}

export interface TuiStreamState {
  /** Current stream status */
  status: "idle" | "running" | "aborting" | "awaiting_approval";
  /** Accumulated assistant text so far */
  assistantText: string;
  /** Whether thinking is active */
  thinkingActive: boolean;
  /** Accumulated thinking text for the current turn */
  thinkingText?: string;
  /** Currently executing tool call id */
  currentToolCallId?: string;
  /** Structured queue state from lifecycle events */
  queue?: StatusQueueContract;
  /** Abort controller for the current stream (not serialized) */
  abortController?: AbortController;
}

export interface TuiApprovalRequest {
  /** Internal identity used for queueing and resolution. */
  toolEntityId: string;
  /** Opaque provider correlation ID. */
  callId: string;
  toolName: string;
  toolArgs: unknown;
}

export interface TuiApprovalState {
  /** Approval currently presented to the user. */
  pending?: TuiApprovalRequest;
  /** FIFO approvals waiting behind the presented request. */
  queue: TuiApprovalRequest[];
}

// ============================================================================
// View state
// ============================================================================

export interface TuiInputState {
  /** Whether the input has focus */
  focused: boolean;
  draft: string;
  revision: number;
  source?:
    | { kind: "user" }
    | { kind: "session_tree"; sessionId: string; entryId: string }
    | { kind: "queue_restore" };
}

// ============================================================================
// Layout state
// ============================================================================

export type LayoutMode = "regular" | "compact" | "minimal";
export type LayoutActiveRegion = "chat" | "editor";
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
  theme: string;
  hideThinking: boolean;
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

  /** Approval state */
  approval: TuiApprovalState;

  /** View state */
  input: TuiInputState;

  /** Layout state (derived from domain/view + viewport) */
  layout: TuiLayoutState;

  /** Extension slot state */
  extensions: TuiExtensionSlots;

  /** Whether the app is currently running (not yet shut down) */
  running: boolean;

  /** The ID of the currently focused agent. */
  currentAgentId: string;
  /** Agent whose timeline/status is being inspected; independent from prompt routing. */
  viewedAgentId: string;
  /** Which agent panel is expanded to show plan steps (undefined = none expanded). */
  expandedAgentId?: string;

  // ---- UX Runtime subsystems ----
  /** In-memory notification history for current session */
  notifications: TuiNotification[];
  /** Active UI surfaces */
  surfaces: SurfaceState[];
  /** Focus ownership state */
  focus: TuiFocusState;
  /** Timeline view state (scroll, expansion, streaming) */
  timeline: TuiTimelineState;

  /** Deterministic timeline projection (ordered IDs + items by ID). */
  projection: TimelineProjection;

  /** Pending scroll command for TimelineView */
  scrollCommand?: { dir: "pageUp" | "pageDown" | "jumpLatest"; seq: number } | null;

  /** Internal counter for scroll command sequencing */
  _scrollSeq: number;
}

// ============================================================================
// Default state factory
// ============================================================================

export function createDefaultTuiState(
  model: Model<string>,
  providerConfig: ModelProviderConfig,
  cwd: string,
  thinkingLevel?: string,
  initialLayout?: Partial<TuiLayoutState>,
): TuiState {
  return {
    session: {
      cwd,
      messageCount: 0,
      navigation: { status: "idle" },
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
      thinkingText: "",
    },
    approval: { queue: [] },
    input: {
      focused: true,
      draft: "",
      revision: 0,
    },
    layout: {
      viewport: { width: 80, height: 24 },
      mode: "regular",
      activeRegion: "editor",
      bottomBar: {
        density: "full",
        visibleFields: ["model", "session", "branch", "tokens", "cost", "cwd", "mode", "hints"],
      },
      theme: initialLayout?.theme ?? "dark",
      hideThinking: initialLayout?.hideThinking ?? false,
    },
    extensions: {
      statusSlots: new Map(),
    },
    running: true,
    currentAgentId: "main",
    viewedAgentId: "main",

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
    projection: createProjection(),
    _scrollSeq: 0,
  };
}
