import type { SessionPersistenceOverview } from "../../session/index.js";
import type { FollowUpMessage, NextTurnMessage, SteeringMessage } from "../shared/index.js";
import { AgentMessageQueue } from "./queue.js";

export class HostState {
  private agentQueues = new Map<string, AgentMessageQueue>();
  private _sessionPersistenceOverview?: SessionPersistenceOverview;

  get sessionPersistenceOverview(): SessionPersistenceOverview | undefined {
    return this._sessionPersistenceOverview;
  }

  setSessionPersistenceOverview(overview: SessionPersistenceOverview): void {
    this._sessionPersistenceOverview = overview;
  }

  resetForSession(overview?: SessionPersistenceOverview): void {
    this.agentQueues.clear();
    this._sessionPersistenceOverview = overview;
  }

  getAgentQueue(agentId: string): AgentMessageQueue {
    let queue = this.agentQueues.get(agentId);
    if (!queue) {
      queue = new AgentMessageQueue();
      this.agentQueues.set(agentId, queue);
    }
    return queue;
  }

  getQueueState(agentId: string): {
    steering: ReadonlyArray<SteeringMessage>;
    followUp: ReadonlyArray<FollowUpMessage>;
    nextTurn: ReadonlyArray<NextTurnMessage>;
  } {
    return this.getAgentQueue(agentId).state;
  }

  dequeue(agentId: string): {
    steering: SteeringMessage[];
    followUp: FollowUpMessage[];
    nextTurn: NextTurnMessage[];
  } {
    return this.getAgentQueue(agentId).dequeue();
  }
}
