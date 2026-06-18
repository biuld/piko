import type { EventStream } from "piko-orchestrator";
import type { HostRuntimeEvent, ImageContent } from "piko-orchestrator-protocol";

import type { HostLifecycleEvent } from "../lifecycle/index.js";
import type {
  FollowUpMessage,
  NextTurnMessage,
  QueueMode,
  SteeringMessage,
  StreamPromptOptions,
  StreamPromptResult,
} from "../shared/index.js";
import type { HostState } from "../state/index.js";

export type PromptBehavior = "auto" | "steer" | "followUp";

export class HostQueueController {
  private lifecycleCallback?: (event: HostLifecycleEvent) => void;

  constructor(
    private readonly state: HostState,
    private readonly isRunning: (agentId: string) => boolean,
    private readonly startStream: (
      text: string,
      options: StreamPromptOptions,
    ) => EventStream<HostRuntimeEvent, StreamPromptResult>,
  ) {}

  setSteeringMode(_mode: QueueMode): void {
    // Steering mode is tracked by SettingsManager; queue consumption is a future feature.
  }

  setFollowUpMode(_mode: QueueMode): void {
    // Follow-up mode is tracked by SettingsManager; queue consumption is a future feature.
  }

  setLifecycleCallback(cb: (event: HostLifecycleEvent) => void): void {
    this.lifecycleCallback = cb;
  }

  steer(text: string, images?: ImageContent[], agentId = "main"): void {
    if (!this.isRunning(agentId)) {
      throw new Error("Cannot steer while idle");
    }
    this.state.getAgentQueue(agentId).pushSteering(text, images);
    this.emitQueueUpdate(agentId);
  }

  followUp(text: string, images?: ImageContent[], agentId = "main"): void {
    if (!this.isRunning(agentId)) {
      throw new Error("Cannot follow up while idle");
    }
    this.state.getAgentQueue(agentId).pushFollowUp(text, images);
    this.emitQueueUpdate(agentId);
  }

  nextTurn(text: string, images?: ImageContent[], agentId = "main"): void {
    this.state.getAgentQueue(agentId).pushNextTurn(text, images);
    this.emitQueueUpdate(agentId);
  }

  prompt(
    text: string,
    behavior: PromptBehavior = "auto",
    agentId = "main",
  ): EventStream<HostRuntimeEvent, StreamPromptResult> | null {
    if (this.isRunning(agentId)) {
      if (behavior === "followUp") {
        this.followUp(text, undefined, agentId);
      } else {
        this.steer(text, undefined, agentId);
      }
      return null;
    }
    return this.startStream(text, { agentId });
  }

  getQueueState(agentId = "main"): {
    steering: ReadonlyArray<SteeringMessage>;
    followUp: ReadonlyArray<FollowUpMessage>;
    nextTurn: ReadonlyArray<NextTurnMessage>;
  } {
    return this.state.getQueueState(agentId);
  }

  dequeue(agentId = "main"): {
    steering: SteeringMessage[];
    followUp: FollowUpMessage[];
    nextTurn: NextTurnMessage[];
  } {
    const result = this.state.dequeue(agentId);
    this.emitQueueUpdate(agentId);
    return result;
  }

  private emitQueueUpdate(agentId = "main"): void {
    if (!this.lifecycleCallback) return;
    const MAX_PREVIEW = 80;
    const queue = this.state.getAgentQueue(agentId);
    const state = queue.state;
    this.lifecycleCallback({
      type: "queue_update",
      agentId,
      steerCount: queue.steeringCount,
      followUpCount: queue.followUpCount,
      nextTurnCount: queue.nextTurnCount,
      steerPreview: state.steering[0]?.text.slice(0, MAX_PREVIEW),
      followUpPreview: state.followUp[0]?.text.slice(0, MAX_PREVIEW),
    });
  }
}
