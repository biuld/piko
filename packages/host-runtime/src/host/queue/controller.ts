import type { EventStream, HostRuntimeEvent, ImageContent } from "piko-orch-protocol";

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
  /**
   * Tracks streams from the moment they are admitted by the Host. The
   * orchestrator snapshot does not become `running` until asynchronous run
   * preparation completes, so relying on it alone leaves a window where a
   * second prompt can incorrectly start another stream for the same agent.
   */
  private readonly activeStreams = new Map<
    string,
    EventStream<HostRuntimeEvent, StreamPromptResult>
  >();

  constructor(
    private readonly state: HostState,
    private readonly isRunning: (agentId: string) => boolean,
    private readonly startStream: (
      text: string,
      options: StreamPromptOptions,
      signal?: AbortSignal,
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
    if (!this.isAgentActive(agentId)) {
      throw new Error("Cannot steer while idle");
    }
    this.state.getAgentQueue(agentId).pushSteering(text, images);
    this.emitQueueUpdate(agentId);
  }

  followUp(text: string, images?: ImageContent[], agentId = "main"): void {
    if (!this.isAgentActive(agentId)) {
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
    signal?: AbortSignal,
  ): EventStream<HostRuntimeEvent, StreamPromptResult> | null {
    if (this.isAgentActive(agentId)) {
      if (behavior === "followUp") {
        this.followUp(text, undefined, agentId);
      } else {
        this.steer(text, undefined, agentId);
      }
      return null;
    }

    const stream = this.startStream(text, { agentId }, signal);
    this.activeStreams.set(agentId, stream);

    const clearIfCurrent = () => {
      if (this.activeStreams.get(agentId) === stream) {
        this.activeStreams.delete(agentId);
      }
    };
    void stream.result().then(clearIfCurrent, clearIfCurrent);

    return stream;
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

  private isAgentActive(agentId: string): boolean {
    return this.activeStreams.has(agentId) || this.isRunning(agentId);
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
