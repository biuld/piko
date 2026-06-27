// ============================================================================
// EventBus — central publish/subscribe bus for unified HostEvents.
//
// Responsibilities:
//   1. Broadcast all events to subscribers (TUI, RPC, extensions)
//   2. Persist domain events to the session JSONL journal
//   3. Provide state rebuild from journal events
// ============================================================================

import type { SessionTreeEntry } from "piko-session";
import type {
  HostEvent,
  HostEventListener,
} from "./protocol/host-event.js";
import { isDomainEvent } from "./protocol/host-event.js";
import type { AssistantMessage, Message } from "./protocol/messages.js";

// ============================================================================
// EventJournal — persists domain events to JSONL session storage
// ============================================================================

export interface EventJournal {
  /** Append a domain event to the journal (persisted as a session entry). */
  append(event: HostEvent): Promise<void>;
  /** Read all events from the journal for state rebuild. */
  readAll(): Promise<HostEvent[]>;
}

/**
 * Journal adapter that writes domain events as custom session entries
 * using the existing piko-session SessionStorage.
 */
export class SessionEventJournal implements EventJournal {
  private cache: HostEvent[] | null = null;

  constructor(
    private readonly getSessionStorage: () => {
      appendEntry: (entry: SessionTreeEntry) => Promise<void>;
      getEntries: () => Promise<SessionTreeEntry[]>;
    } | null,
  ) {}

  async append(event: HostEvent): Promise<void> {
    const storage = this.getSessionStorage();
    if (!storage) return;

    // Invalidate cache
    this.cache = null;

    const entry: SessionTreeEntry = {
      type: "custom",
      customType: `host_event.${event.type}`,
      id: `hev-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      parentId: null,
      timestamp: String(("timestamp" in event ? (event as { timestamp: number }).timestamp : undefined) ?? Date.now()),
      data: event,
    } as SessionTreeEntry;

    await storage.appendEntry(entry);
  }

  async readAll(): Promise<HostEvent[]> {
    if (this.cache) return this.cache;

    const storage = this.getSessionStorage();
    if (!storage) return [];

    const entries = await storage.getEntries();
    const events: HostEvent[] = [];

    for (const entry of entries) {
      if (
        entry.type === "custom" &&
        "customType" in entry &&
        typeof entry.customType === "string" &&
        entry.customType.startsWith("host_event.") &&
        "data" in entry
      ) {
        events.push((entry as { data: HostEvent }).data);
      }
    }

    this.cache = events;
    return events;
  }

  /** Invalidate the read cache (call after external writes). */
  invalidateCache(): void {
    this.cache = null;
  }
}

// ============================================================================
// EventBus
// ============================================================================

export class EventBus {
  private subscribers = new Set<HostEventListener>();
  private journal: EventJournal | null = null;

  /** Attach a journal for domain event persistence. */
  setJournal(journal: EventJournal | null): void {
    this.journal = journal;
  }

  getJournal(): EventJournal | null {
    return this.journal;
  }

  /**
   * Publish an event.
   * - Domain events are persisted to the journal (async, fire-and-forget).
   * - All events are broadcast synchronously to subscribers.
   */
  publish(event: HostEvent): void {
    // Persist domain events to journal
    if (isDomainEvent(event) && this.journal) {
      const journal = this.journal;
      // Fire and forget — don't block subscribers
      journal.append(event).catch((err) => {
        console.error("[EventBus] Failed to persist domain event:", err);
      });
    }

    // Broadcast to all subscribers
    for (const listener of this.subscribers) {
      try {
        listener(event);
      } catch (err) {
        console.error("[EventBus] Subscriber error:", err);
      }
    }
  }

  /**
   * Subscribe to all events.
   * Returns an unsubscribe function.
   */
  subscribe(listener: HostEventListener): () => void {
    this.subscribers.add(listener);
    return () => {
      this.subscribers.delete(listener);
    };
  }

  /** Number of active subscribers. */
  get subscriberCount(): number {
    return this.subscribers.size;
  }
}

// ============================================================================
// Session state rebuild from domain events
// ============================================================================

export interface RebuiltSessionState {
  sessionId: string;
  cwd: string;
  activeTurnId: string | null;
  activeTasks: Map<string, RebuiltTaskState>;
  messages: RebuiltTranscriptMessage[];
}

export interface RebuiltTaskState {
  agentId: string;
  parentTaskId: string | null;
  status: "running" | "completed" | "failed" | "cancelled";
}

export interface RebuiltTranscriptMessage {
  id: string;
  role: "user" | "assistant" | "toolResult";
  text: string;
  toolCalls?: Array<{ id: string; name: string; args: unknown }>;
}

/**
 * Rebuild session state from a sequence of domain events (e.g., from journal replay).
 * This is a pure function — no side effects.
 */
export function rebuildSessionState(events: HostEvent[]): RebuiltSessionState {
  const state: RebuiltSessionState = {
    sessionId: "",
    cwd: "",
    activeTurnId: null,
    activeTasks: new Map(),
    messages: [],
  };

  for (const event of events) {
    switch (event.type) {
      case "session_created": {
        state.sessionId = event.session_id;
        state.cwd = event.cwd;
        break;
      }
      case "turn_started": {
        state.activeTurnId = event.turn_id;
        break;
      }
      case "turn_completed":
      case "turn_failed":
      case "turn_cancelled": {
        if (state.activeTurnId === event.turn_id) {
          state.activeTurnId = null;
        }
        break;
      }
      case "task_started": {
        state.activeTasks.set(event.task_id, {
          agentId: event.agent_id,
          parentTaskId: null, // will be updated by task_created if available
          status: "running",
        });
        break;
      }
      case "task_created": {
        const existing = state.activeTasks.get(event.task_id);
        state.activeTasks.set(event.task_id, {
          agentId: event.agent_id,
          parentTaskId: event.parent_task_id,
          status: existing?.status ?? "running",
        });
        break;
      }
      case "task_completed": {
        const task = state.activeTasks.get(event.task_id);
        if (task) {
          task.status = "completed";
        }
        break;
      }
      case "task_failed": {
        const task = state.activeTasks.get(event.task_id);
        if (task) {
          task.status = "failed";
        }
        break;
      }
      case "task_cancelled": {
        const task = state.activeTasks.get(event.task_id);
        if (task) {
          task.status = "cancelled";
        }
        break;
      }
      case "user_message_submitted": {
        state.messages.push({
          id: event.message_id,
          role: "user",
          text: event.text,
        });
        break;
      }
      case "assistant_message_completed": {
        state.messages.push({
          id: event.message_id,
          role: "assistant",
          text: event.text,
          toolCalls: event.tool_calls.map((tc: { id: string; name: string; args: unknown }) => ({
            id: tc.id,
            name: tc.name,
            args: tc.args,
          })),
        });
        break;
      }
      case "tool_result_committed": {
        state.messages.push({
          id: event.message_id,
          role: "toolResult",
          text:
            typeof event.content === "string"
              ? event.content
              : JSON.stringify(event.content),
        });
        break;
      }
      // task_steered, task_transcript_committed, task_joined, queue_update,
      // model_config_changed are not directly reflected in simplified state.
      default:
        break;
    }
  }

  return state;
}

/**
 * Convert domain events to pi-ai Message format for agent replay.
 */
export function eventsToMessages(events: HostEvent[]): Message[] {
  const messages: Message[] = [];

  for (const event of events) {
    switch (event.type) {
      case "user_message_submitted": {
        messages.push({
          role: "user",
          content: event.text,
          timestamp: event.timestamp,
        });
        break;
      }
      case "assistant_message_completed": {
        const content: Array<{ type: "text"; text: string } | { type: "toolCall"; id: string; name: string; arguments: Record<string, unknown> }> = [
          { type: "text", text: event.text },
        ];
        for (const tc of event.tool_calls) {
          content.push({
            type: "toolCall",
            id: tc.id,
            name: tc.name,
            arguments: tc.args as Record<string, unknown>,
          });
        }
        messages.push({
          role: "assistant",
          content: content as AssistantMessage["content"],
          api: "openai-completions",
          provider: event.provider,
          model: event.model,
          usage: event.usage ?? {
            input: 0,
            output: 0,
            cacheRead: 0,
            cacheWrite: 0,
            totalTokens: 0,
            cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
          },
          stopReason: "stop",
          timestamp: event.timestamp,
        });
        break;
      }
      case "tool_result_committed": {
        messages.push({
          role: "toolResult",
          toolCallId: event.tool_call_id,
          toolName: event.tool_name,
          content: [
            {
              type: "text" as const,
              text:
                typeof event.content === "string"
                  ? event.content
                  : JSON.stringify(event.content),
            },
          ],
          details: event.content,
          isError: event.is_error,
          timestamp: event.timestamp,
        });
        break;
      }
      default:
        break;
    }
  }

  return messages;
}
