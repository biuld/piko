import type { ImageContent, Message, PendingApprovalState } from "piko-engine-protocol";

export type SessionRunState =
  | "idle"
  | "running"
  | "awaiting_approval"
  | "completed"
  | "aborted"
  | "error";

export interface SessionState {
  sessionId: string;
  messages: Message[];
  systemPrompt: string;
  createdAt: number;
  updatedAt: number;
  runState: SessionRunState;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}

export interface CreateSessionStateOptions {
  sessionId?: string;
  messages?: Message[];
  systemPrompt: string;
  createdAt?: number;
  updatedAt?: number;
  runState?: SessionRunState;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}

export function createSession(options: CreateSessionStateOptions): SessionState {
  const sessionId =
    options.sessionId ?? `session-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  return {
    sessionId,
    messages: options.messages ?? [],
    systemPrompt: options.systemPrompt,
    createdAt: options.createdAt ?? Date.now(),
    updatedAt: options.updatedAt ?? Date.now(),
    runState: options.runState ?? "idle",
    pendingApproval: options.pendingApproval,
    engineState: options.engineState,
  };
}

export function appendMessages(session: SessionState, messages: Message[]): SessionState {
  return {
    ...session,
    messages: [...session.messages, ...messages],
    updatedAt: Date.now(),
  };
}

export function updateSessionState(
  session: SessionState,
  updates: Partial<Pick<SessionState, "runState" | "pendingApproval" | "engineState">>,
): SessionState {
  return {
    ...session,
    ...updates,
    updatedAt: Date.now(),
  };
}

export function addUserMessage(
  session: SessionState,
  content: string,
  images?: ImageContent[],
): SessionState {
  const userMsg: Message = {
    role: "user",
    content:
      images && images.length > 0 ? [{ type: "text" as const, text: content }, ...images] : content,
    timestamp: Date.now(),
  };
  return appendMessages(
    updateSessionState(session, {
      runState: "idle",
      pendingApproval: undefined,
    }),
    [userMsg],
  );
}
