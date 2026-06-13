import type { Message, Model } from "@earendil-works/pi-ai";
import type {
  EngineProviderConfig,
  EngineRunSettings,
  EngineStepStatus,
  StopReason,
  ToolDef,
  TranscriptDelta,
} from "piko-protocol";

export type ActiveToolsState = { kind: "all" } | { kind: "only"; names: string[] };

export function activeToolsStateFromNames(toolNames: string[] | undefined): ActiveToolsState {
  return toolNames && toolNames.length > 0
    ? { kind: "only", names: [...toolNames] }
    : { kind: "all" };
}

export function activeToolNamesFromState(state: ActiveToolsState): string[] | undefined {
  return state.kind === "only" ? [...state.names] : undefined;
}

/**
 * Full turn snapshot built before each engine step.
 *
 * This replaces the previous TurnPreparation (which was just overrides)
 * with a complete picture of the current runtime state. Consumers can
 * inspect the TurnState to understand exactly what model, tools, thinking
 * level, system prompt, auth, and settings were used for a given turn.
 *
 * Built per-turn so that mid-run changes (model/thinking/tools/settings)
 * are picked up without restarting the entire run.
 */
export interface TurnState {
  /** 0-based index of the turn within the current run. */
  turnIndex: number;

  /** Session messages at turn start (transcript context for the LLM). */
  messages: Message[];

  /** System prompt used for this turn. May change per turn if resources/skills change. */
  systemPrompt: string;

  /** The model selected for this turn. */
  model: Model<string>;

  /** Provider configuration including API key, headers, base URL. */
  provider: EngineProviderConfig;

  /** Thinking/reasoning level for this turn (undefined = "off"). */
  thinkingLevel?: string;

  /** All registered tools (regardless of whether they're active). */
  allTools: ToolDef[];

  /** The subset of tools actively available to the LLM for this turn. */
  activeTools: ToolDef[];

  /** Engine run settings (maxSteps, parallelTools, thinkingLevel, etc.). */
  settings: EngineRunSettings;
}

/**
 * Snapshot of what happened during a single turn.
 *
 * Captures the TurnState that was used plus the outcomes:
 * appended messages, engine state changes, pending approvals, stop reason.
 *
 * This enables consumers (TUI, session storage, RPC) to understand the
 * full context of a completed turn without replaying events.
 */
export interface TurnResult {
  /** The TurnState that was active for this turn. */
  turnState: TurnState;

  /** Terminal status of this turn (continue, completed, awaiting_approval, error, aborted). */
  status: EngineStepStatus;

  /** Messages that were appended to the session during this turn. */
  appendedMessages: Message[];

  /** Opaque engine state after this turn (for engines that maintain state). */
  engineState?: unknown;

  /** Pending approval, if the turn stopped awaiting user decision. */

  /** Why the engine stopped (assistant, tool, max_steps, approval, abort, error). */
  stopReason?: StopReason;

  /** Durable transcript deltas for persistence (canonical over appendedMessages). */
  transcriptDelta?: TranscriptDelta[];
}

/**
 * Callback signature for per-turn state construction.
 *
 * Receives the current session and turn metadata, returns the TurnState
 * to use for the upcoming step. Consumers may return synchronously or
 * asynchronously (e.g., to refresh auth tokens or rebuild system prompts).
 *
 * Replaces the previous `TurnPreparation` callback (which only returned
 * partial overrides).
 */
export type PrepareTurnFn = (ctx: TurnBuildContext) => TurnState | Promise<TurnState>;

/**
 * Context passed to PrepareTurnFn for building a TurnState.
 */
export interface TurnBuildContext {
  /** The current session state (messages, systemPrompt, pendingApproval, engineState). */
  session: import("./session/index.js").SessionState;
  /** Status of the previous turn ("continue" after tool results, "completed" to break, etc.). */
  previousStatus: EngineStepStatus;
  /** 0-based index of the upcoming turn. */
  turnIndex: number;
  /** Total engine steps executed so far across all turns. */
  totalSteps: number;
}
