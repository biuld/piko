import type { EngineEvent, EventStream, StatelessEngine } from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { HostConfig } from "../models/index.js";
import type {
  FollowUpMessage,
  NextTurnMessage,
  QueueMode,
  SteeringMessage,
  TurnContext,
  TurnPreparation,
} from "../scheduler.js";
import { runScheduler } from "../scheduler.js";
import type { SessionManager } from "../session/index.js";
import { addUserMessage, createSession } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";
import { runMaybeCompact } from "./compaction.js";
import type { HostRunResult, StreamPromptOptions, StreamPromptResult } from "./types.js";

/**
 * Create a prepareTurn callback that dynamically reads the host's mutable config.
 * Unlike the old closure-based approach, this picks up model/thinking changes
 * made mid-run (e.g., via TUI Ctrl+P/N or /model).
 */
export function createPrepareNextTurn(
  getConfig: () => HostConfig,
  getThinkingLevel: () => string,
): (ctx: TurnContext) => TurnPreparation {
  return (_ctx) => {
    const config = getConfig();
    const thinkingLevel = getThinkingLevel();
    return {
      model: config.model,
      provider: config.provider,
      thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
    };
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
  prepareTurn?: (ctx: TurnContext) => TurnPreparation | Promise<TurnPreparation>,
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
  prepareTurn?: (ctx: TurnContext) => TurnPreparation | Promise<TurnPreparation>,
  signal?: AbortSignal,
  steeringMode?: QueueMode,
  followUpMode?: QueueMode,
): EventStream<EngineEvent, StreamPromptResult> {
  const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();

  void loadSessionState(sessionManager, systemPrompt)
    .then(async (session) => {
      const nextSession = addUserMessage(session, prompt);
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
