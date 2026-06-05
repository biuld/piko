import type { EngineEvent, EventStream, StatelessEngine } from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type {
  FollowUpMessage,
  NextTurnMessage,
  QueueMode,
  SteeringMessage,
} from "../loop/index.js";
import { runScheduler } from "../loop/index.js";
import type { HostConfig } from "../models/index.js";
import type { SessionManager } from "../session/index.js";
import { addUserMessage, createSession } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";
import type {
  ActiveToolsState,
  PrepareTurnFn,
  TurnBuildContext,
  TurnState,
} from "../turn-state.js";
import { runMaybeCompact } from "./compaction.js";
import type { HostRunResult, StreamPromptOptions, StreamPromptResult } from "./types.js";

/**
 * Create a prepareTurn callback that dynamically reads the host's mutable config
 * and builds a full TurnState snapshot per turn.
 *
 * Unlike the old closure-based approach (which only returned model/provider/thinking overrides),
 * this builds a complete TurnState with messages, systemPrompt, tools, activeTools, and settings
 * derived from the current host state. System prompt is rebuilt per turn from the host's
 * config so mid-run changes (e.g. via /reload) are picked up.
 */
export function createPrepareNextTurn(
  getConfig: () => HostConfig,
  getThinkingLevel: () => string,
  getSystemPrompt?: () => string,
  getActiveToolsState?: () => ActiveToolsState,
): PrepareTurnFn {
  return (ctx: TurnBuildContext) => {
    const config = getConfig();
    const thinkingLevel = getThinkingLevel();
    const effectiveThinking = thinkingLevel !== "off" ? thinkingLevel : undefined;
    const allTools = config.tools ?? [];
    const systemPrompt = getSystemPrompt ? getSystemPrompt() : ctx.session.systemPrompt;

    const activeToolsState = getActiveToolsState?.() ?? { kind: "all" };
    const activeTools =
      activeToolsState.kind === "only"
        ? allTools.filter((t) => activeToolsState.names.includes(t.name))
        : allTools;

    const turnState: TurnState = {
      turnIndex: ctx.turnIndex,
      messages: ctx.session.messages,
      systemPrompt,
      model: config.model,
      provider: config.provider,
      thinkingLevel: effectiveThinking,
      allTools,
      activeTools,
      settings: {
        ...config.settings,
        ...(effectiveThinking ? { thinkingLevel: effectiveThinking } : {}),
      },
    };
    return turnState;
  };
}

export function getRetryConfig(
  settingsManager?: SettingsManager,
): { maxRetries: number; baseDelayMs: number } | undefined {
  if (settingsManager) {
    const r = settingsManager.getRetrySettings();
    if (r.enabled) return { maxRetries: r.maxRetries, baseDelayMs: r.baseDelayMs };
    return undefined;
  }
  return { maxRetries: 1, baseDelayMs: 2000 };
}

export async function loadSessionState(
  sessionManager: SessionManager,
  systemPrompt: string,
): Promise<ReturnType<typeof createSession>> {
  const existingMessages = await sessionManager.loadMessages();
  return createSession({
    sessionId: sessionManager.getSessionId(),
    messages: existingMessages,
    systemPrompt,
  });
}

export async function runHostPrompt(
  engine: StatelessEngine,
  config: HostConfig,
  sessionManager: SessionManager,
  systemPrompt: string,
  settingsManager: SettingsManager | undefined,
  approvalHandler: ApprovalHandler | undefined,
  steeringQueue: SteeringMessage[],
  followUpQueue: FollowUpMessage[],
  nextTurnQueue: NextTurnMessage[],
  prompt: string,
  prepareTurn?: PrepareTurnFn,
  signal?: AbortSignal,
  steeringMode?: QueueMode,
  followUpMode?: QueueMode,
): Promise<HostRunResult> {
  const loadedSession = await loadSessionState(sessionManager, systemPrompt);
  const session = addUserMessage(loadedSession, prompt);

  const result = await runScheduler({
    engine,
    config,
    session,
    approvalHandler,
    signal,
    retry: getRetryConfig(settingsManager),
    prepareTurn,
    steeringQueue,
    followUpQueue,
    nextTurnQueue,
    steeringMode,
    followUpMode,
    onSavePoint: async (s) => {
      await sessionManager.saveMessages(config.model.id, s.messages);
    },
    onMessageFlush: async (s) => {
      await sessionManager.saveMessages(config.model.id, s.messages);
    },
  });

  await sessionManager.saveMessages(config.model.id, result.session.messages);
  runMaybeCompact(sessionManager, config, settingsManager).catch(() => {});

  return {
    messages: result.session.messages,
    totalSteps: result.totalSteps,
    status: result.status,
    sessionId: sessionManager.getSessionId(),
    sessionFile: sessionManager.getSessionFile(),
  };
}

export function streamHostPrompt(
  engine: StatelessEngine,
  config: HostConfig,
  sessionManager: SessionManager,
  systemPrompt: string,
  settingsManager: SettingsManager | undefined,
  approvalHandler: ApprovalHandler | undefined,
  steeringQueue: SteeringMessage[],
  followUpQueue: FollowUpMessage[],
  nextTurnQueue: NextTurnMessage[],
  prompt: string,
  options: StreamPromptOptions = {},
  prepareTurn?: PrepareTurnFn,
  signal?: AbortSignal,
  steeringMode?: QueueMode,
  followUpMode?: QueueMode,
): EventStream<EngineEvent, StreamPromptResult> {
  const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();

  void loadSessionState(sessionManager, systemPrompt)
    .then(async (session) => {
      const nextSession = addUserMessage(session, prompt, options.images);
      const result = await runScheduler({
        engine,
        config: {
          ...config,
          settings: { ...config.settings, ...options.settingsOverride },
        },
        session: nextSession,
        approvalHandler,
        signal,
        retry: getRetryConfig(settingsManager),
        prepareTurn,
        steeringQueue,
        followUpQueue,
        nextTurnQueue,
        steeringMode,
        followUpMode,
        onSavePoint: async (s) => {
          await sessionManager.saveMessages(config.model.id, s.messages);
        },
        onMessageFlush: async (s) => {
          await sessionManager.saveMessages(config.model.id, s.messages);
        },
        onEvent: (event) => {
          stream.push(event);
        },
        onLifecycleEvent: options.onLifecycleEvent,
      });

      await sessionManager.saveMessages(config.model.id, result.session.messages);
      runMaybeCompact(sessionManager, config, settingsManager).catch(() => {});

      const appendedMessages = result.session.messages.slice(nextSession.messages.length);
      stream.end({
        messages: result.session.messages,
        appendedMessages,
        status: result.status,
        sessionId: sessionManager.getSessionId(),
        sessionFile: sessionManager.getSessionFile(),
      });
    })
    .catch((err) => {
      const message = err instanceof Error ? err.message : String(err);
      stream.push({ type: "error", message });
      stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
    });

  return stream;
}
