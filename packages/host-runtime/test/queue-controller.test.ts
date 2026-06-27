import { describe, expect, it } from "bun:test";
import { HostQueueController } from "../src/host/queue/controller.js";
import type { StreamPromptResult } from "../src/host/shared/index.js";
import { HostState } from "../src/host/state/index.js";
import { EventStream, type HostEvent } from "../src/orchd/protocol/index.js";

function createStream(): EventStream<HostEvent, StreamPromptResult> {
  return new EventStream<HostEvent, StreamPromptResult>();
}

function completedResult(): StreamPromptResult {
  return {
    messages: [],
    appendedMessages: [],
    status: "completed",
    sessionId: "session-test",
  };
}

describe("HostQueueController", () => {
  it("queues a second prompt during asynchronous run admission", async () => {
    const streams: Array<EventStream<HostEvent, StreamPromptResult>> = [];
    const controller = new HostQueueController(
      new HostState(),
      () => false,
      () => "test-session",
      () => {
        const stream = createStream();
        streams.push(stream);
        return stream;
      },
    );

    const stream1 = controller.prompt("first");
    expect(stream1).not.toBeNull();

    const stream2 = controller.prompt("second");
    expect(stream2).toBeNull();

    const queueState = controller.getQueueState();
    expect(queueState.steering.length).toBe(1);
    expect(queueState.steering[0].text).toBe("second");
  });
});
