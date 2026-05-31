import type {
  EngineEvent,
  EngineTool,
  EventStream,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { HostConfig } from "../models/index.js";
import type { TurnPreparation } from "../scheduler.js";
import { runScheduler } from "../scheduler.js";
import type { SettingsManager } from "../settings/index.js";
import type { SessionManager } from "../session/index.js";
import { addUserMessage, createSession } from "../session/index.js";
import type { StreamPromptOptions, StreamPromptResult, HostRunResult } from "./types.js";
import type { SteeringMessage, FollowUpMessage, NextTurnMessage } from "../scheduler.js";
import { runMaybeCompact } from "./compaction.js";

export function buildPrepareTurnFn(
  config: HostConfig,
  thinkingLevel: string,
): () => TurnPreparation {
  return () => ({
    model: config.model,
    provider: config.provider,
    thinkingLevel: thinkingLevel !== "off" ? thinkingLevel : undefined,
  });
}

export function getRetryConfig(settingsManager?: SettingsManager): { maxRetries: number; baseDelayMs: number } | undefined {
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
  thinkingLevel: string,
  steeringQueue: SteeringMessage[],
  followUpQueue: FollowUpMessage[],
  nextTurnQueue: NextTurnMessage[],
  prompt: string,
  signal?: AbortSignal,
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
    prepareTurn: buildPrepareTurnFn(config, thinkingLevel),
    steeringQueue,
    followUpQueue,
    nextTurnQueue,
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
  thinkingLevel: string,
  steeringQueue: SteeringMessage[],
  followUpQueue: FollowUpMessage[],
  nextTurnQueue: NextTurnMessage[],
  prompt: string,
  options: StreamPromptOptions = {},
  signal?: AbortSignal,
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
        prepareTurn: buildPrepareTurnFn(config, thinkingLevel),
        steeringQueue,
        followUpQueue,
        nextTurnQueue,
        onEvent: (event) => { stream.push(event); },
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
