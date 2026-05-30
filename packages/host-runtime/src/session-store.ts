import type { Message } from "@earendil-works/pi-ai";

export interface SessionState {
  sessionId: string;
  messages: Message[];
  systemPrompt: string;
  createdAt: number;
  updatedAt: number;
}

export function createSession(systemPrompt: string): SessionState {
  const sessionId = `session-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  return {
    sessionId,
    messages: [],
    systemPrompt,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  };
}

export function appendMessages(
  session: SessionState,
  messages: Message[],
): SessionState {
  return {
    ...session,
    messages: [...session.messages, ...messages],
    updatedAt: Date.now(),
  };
}

export function addUserMessage(
  session: SessionState,
  content: string,
): SessionState {
  const userMsg: Message = {
    role: "user",
    content,
    timestamp: Date.now(),
  };
  return appendMessages(session, [userMsg]);
}
