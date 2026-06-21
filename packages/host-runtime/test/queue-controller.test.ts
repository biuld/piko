import { describe, expect, it } from "bun:test";
import { EventStream } from "piko-orchestrator";
import type { HostRuntimeEvent } from "piko-orchestrator-protocol";
import { HostQueueController } from "../src/host/queue/controller.js";
import type { StreamPromptResult } from "../src/host/shared/index.js";
import { HostState } from "../src/host/state/index.js";

function createStream(): EventStream<HostRuntimeEvent, StreamPromptResult> {
  return new EventStream<HostRuntimeEvent, StreamPromptResult>();
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
    const streams: Array<EventStream<HostRuntimeEvent, StreamPromptResult>> = [];
    const controller = new HostQueueController(
      new HostState(),
      () => false,
      () => {
        const stream = createStream();
        streams.push(stream);
        return stream;
      },
    );

    const first = controller.prompt("first");
    expect(first).toBe(streams[0]);

    const second = controller.prompt("steer while preparing");
    expect(second).toBeNull();
    expect(streams).toHaveLength(1);
    expect(controller.getQueueState().steering.map((message) => message.text)).toEqual([
      "steer while preparing",
    ]);

    streams[0].end(completedResult());
    await streams[0].result();
    await Promise.resolve();

    const third = controller.prompt("new run after settlement");
    expect(third).toBe(streams[1]);
    expect(streams).toHaveLength(2);
  });

  it("accepts follow-up messages while a stream is being prepared", () => {
    const stream = createStream();
    const controller = new HostQueueController(
      new HostState(),
      () => false,
      () => stream,
    );

    expect(controller.prompt("first")).toBe(stream);
    expect(controller.prompt("later", "followUp")).toBeNull();
    expect(controller.getQueueState().followUp.map((message) => message.text)).toEqual(["later"]);

    stream.end(completedResult());
  });
});
