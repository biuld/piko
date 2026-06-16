import type { Message } from "piko-orchestrator-protocol";
import type { SessionManager } from "../session/index.js";
import { AgentMessageQueue } from "./queue.js";

/**
 * Per-agent state maintained by the Host.
 *
 * Each agent gets its own message queue (steering / follow-up / next-turn)
 * and its own session context (history loading / message saving).
 *
 * - "main" agent: backed by a real SessionManager → reads/writes JSONL
 * - Sub-agents: ephemeral → no session persistence (results flow back
 *   to the parent agent via tool_result)
 */
export class HostAgentState {
  readonly queue: AgentMessageQueue;
  private readonly _sessionManager?: SessionManager;

  constructor(sessionManager?: SessionManager) {
    this.queue = new AgentMessageQueue();
    this._sessionManager = sessionManager;
  }

  /** Whether this agent's session is backed by a persistent SessionManager. */
  get isPersistent(): boolean {
    return this._sessionManager !== undefined;
  }

  // ---- Session ----

  /**
   * Load conversation history for this agent.
   * - Persistent agents: reads from JSONL session
   * - Ephemeral agents: returns empty array (context comes from delegation prompt)
   */
  async loadHistory(): Promise<Message[]> {
    if (this._sessionManager) {
      return this._sessionManager.loadMessages();
    }
    return [];
  }

  /**
   * Save messages for this agent.
   * - Persistent agents: writes to JSONL session
   * - Ephemeral agents: no-op (results flow back via tool_result)
   */
  async saveMessages(modelId: string, messages: Message[]): Promise<void> {
    if (this._sessionManager) {
      await this._sessionManager.saveMessages(modelId, messages);
    }
  }
}
