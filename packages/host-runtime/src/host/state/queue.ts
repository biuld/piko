import type { ImageContent } from "piko-orch-protocol";
import type { FollowUpMessage, NextTurnMessage, SteeringMessage } from "../shared/index.js";

export class AgentMessageQueue {
  private steering: SteeringMessage[] = [];
  private followUp: FollowUpMessage[] = [];
  private nextTurn: NextTurnMessage[] = [];

  pushSteering(text: string, images?: ImageContent[]): void {
    this.steering.push({ text, images });
  }

  pushFollowUp(text: string, images?: ImageContent[]): void {
    this.followUp.push({ text, images });
  }

  pushNextTurn(text: string, images?: ImageContent[]): void {
    this.nextTurn.push({ text, images });
  }

  get steeringCount(): number {
    return this.steering.length;
  }

  get followUpCount(): number {
    return this.followUp.length;
  }

  get nextTurnCount(): number {
    return this.nextTurn.length;
  }

  get length(): number {
    return this.steering.length + this.followUp.length + this.nextTurn.length;
  }

  get state() {
    return {
      steering: [...this.steering],
      followUp: [...this.followUp],
      nextTurn: [...this.nextTurn],
    };
  }

  dequeue() {
    const steering = this.steering.splice(0);
    const followUp = this.followUp.splice(0);
    const nextTurn = this.nextTurn.splice(0);
    return { steering, followUp, nextTurn };
  }
}
