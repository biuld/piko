// ---- Kernel: ActorSystem tests ----

import { describe, expect, it } from "bun:test";
import type { ActorHandler, ActorRef } from "../../src/kernel/actor-system.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";
import {
  ActorNotFoundError,
  ActorStoppedError,
  AskTimeoutError,
  MailboxFullError,
} from "../../src/kernel/errors.js";
import { sleep } from "../helpers/index.js";

// ---- Helper: create a simple echo actor ----

function echoHandler(): ActorHandler<unknown> {
  return async (msg, ctx, meta) => {
    ctx.reply(meta, { echoed: msg });
  };
}

// ---- Helper: create a counter actor ----

function _counterHandler(): { handler: ActorHandler<{ type: string }>; getCount: () => number } {
  let count = 0;
  return {
    getCount: () => count,
    handler: async (msg, ctx, meta) => {
      if ((msg as { type: string }).type === "inc") {
        count++;
        ctx.reply(meta, { count });
      } else if ((msg as { type: string }).type === "get") {
        ctx.reply(meta, { count });
      }
    },
  };
}

describe("ActorSystem", () => {
  // ---- Spawn / Stop ----

  it("spawns an actor and returns an ActorRef", () => {
    const system = new ActorSystem();
    const ref = system.spawn({
      id: "test-1",
      kind: "test",
      handler: echoHandler() as ActorHandler,
    });
    expect(ref.id).toBe("test-1");
    expect(ref.kind).toBe("test");
    expect(system.hasActor("test-1")).toBe(true);
  });

  it("spawning same id twice returns existing ref (idempotent)", () => {
    const system = new ActorSystem();
    const ref1 = system.spawn({
      id: "test-1",
      kind: "test",
      handler: echoHandler() as ActorHandler,
    });
    const ref2 = system.spawn({
      id: "test-1",
      kind: "test",
      handler: echoHandler() as ActorHandler,
    });
    expect(ref1).toBe(ref2);
  });

  it("stop removes the actor and rejects pending asks", async () => {
    const system = new ActorSystem();

    // Create a blocking actor
    let resolveBlock: () => void = () => {};
    const block = new Promise<void>((r) => {
      resolveBlock = r;
    });
    system.spawn({
      id: "blocker",
      handler: (async (_msg, _ctx, meta) => {
        await block;
        _ctx.reply(meta, "ok");
      }) as ActorHandler,
    });

    // Send ask that will be pending
    const askPromise = system.ask("blocker", { type: "ping" });

    // Stop the actor
    await system.stop("blocker");

    // Pending ask should be rejected
    await expect(askPromise).rejects.toBeInstanceOf(ActorStoppedError);
    expect(system.hasActor("blocker")).toBe(false);
    resolveBlock?.();
  });

  it("stopAll stops all actors", async () => {
    const system = new ActorSystem();
    system.spawn({ id: "a", handler: echoHandler() as ActorHandler });
    system.spawn({ id: "b", handler: echoHandler() as ActorHandler });

    await system.stopAll();
    expect(system.hasActor("a")).toBe(false);
    expect(system.hasActor("b")).toBe(false);
  });

  // ---- send / ask / reply ----

  it("send delivers a message to the actor", async () => {
    const system = new ActorSystem();
    const received: unknown[] = [];

    system.spawn({
      id: "receiver",
      handler: (async (msg, _ctx, _meta) => {
        received.push(msg);
      }) as ActorHandler,
    });

    system.send("receiver", { type: "hello" });
    await sleep(10);

    expect(received.length).toBe(1);
    expect(received[0]).toEqual({ type: "hello" });
  });

  it("ask returns the reply from the actor", async () => {
    const system = new ActorSystem();
    system.spawn({ id: "echo", handler: echoHandler() as ActorHandler });

    const result = await system.ask("echo", { type: "ping" });
    expect(result).toEqual({ echoed: { type: "ping" } });
  });

  it("ask rejects when actor calls ctx.reject", async () => {
    const system = new ActorSystem();
    system.spawn({
      id: "rejector",
      handler: (async (_msg, _ctx, meta) => {
        _ctx.reject(meta, new Error("intentional reject"));
      }) as ActorHandler,
    });

    await expect(system.ask("rejector", { type: "ping" })).rejects.toThrow("intentional reject");
  });

  it("ask times out with AskTimeoutError", async () => {
    const system = new ActorSystem({ defaultMailboxCapacity: 10 });

    // Create a blocking actor
    system.spawn({
      id: "slow",
      handler: (async (_msg, _ctx, _meta) => {
        await sleep(200);
      }) as ActorHandler,
    });

    await expect(
      system.ask("slow", { type: "ping" }, undefined, { deadlineMs: 10 }),
    ).rejects.toBeInstanceOf(AskTimeoutError);
  });

  it("ask to non-existent actor throws ActorNotFoundError", () => {
    const system = new ActorSystem();
    // ask() throws synchronously when the actor doesn't exist
    expect(() => system.ask("ghost", { type: "ping" })).toThrow(ActorNotFoundError);
  });

  it("send to non-existent actor throws ActorNotFoundError", () => {
    const system = new ActorSystem();
    expect(() => system.send("ghost", { type: "ping" })).toThrow(ActorNotFoundError);
  });

  it("send to full mailbox throws MailboxFullError", () => {
    const system = new ActorSystem({ defaultMailboxCapacity: 2 });

    system.spawn({
      id: "full",
      handler: (async () => {
        await sleep(200);
      }) as ActorHandler,
    });

    // e1 is consumed immediately by handler, queue empty
    system.send("full", { type: "msg1" });
    // e2 goes to queue (1 in queue)
    system.send("full", { type: "msg2" });
    // e3 goes to queue (2 in queue = capacity full)
    system.send("full", { type: "msg3" });
    // e4 should throw because queue is full
    expect(() => system.send("full", { type: "msg4" })).toThrow(MailboxFullError);
  });

  // ---- Handler error handling ----

  it("handler errors are caught and reject pending ask", async () => {
    const system = new ActorSystem();
    system.spawn({
      id: "bad",
      handler: (async () => {
        throw new Error("handler boom");
      }) as ActorHandler,
    });

    // The ask should reject since the handler threw
    await expect(system.ask("bad", { type: "ping" })).rejects.toThrow("handler boom");
  });

  it("handler errors for send (fire-and-forget) do not crash the system", async () => {
    const system = new ActorSystem();

    system.spawn({
      id: "bad-send",
      handler: (async () => {
        throw new Error("handler boom for send");
      }) as ActorHandler,
    });

    // send should not throw (error is caught by kernel)
    expect(() => system.send("bad-send", { type: "ping" })).not.toThrow();

    // System should still be functional
    system.spawn({ id: "ok", handler: echoHandler() as ActorHandler });
    const result = await system.ask("ok", { type: "still-works" });
    expect(result).toEqual({ echoed: { type: "still-works" } });
  });

  // ---- Concurrency ----

  it("different actors can process messages concurrently", async () => {
    const system = new ActorSystem();
    const timeline: string[] = [];

    // Two actors that record when they start/end
    system.spawn({
      id: "actor-a",
      handler: (async (msg, _ctx, _meta) => {
        timeline.push(`a-start:${(msg as { n: number }).n}`);
        await sleep(20);
        timeline.push(`a-end:${(msg as { n: number }).n}`);
      }) as ActorHandler,
    });

    system.spawn({
      id: "actor-b",
      handler: (async (msg, _ctx, _meta) => {
        timeline.push(`b-start:${(msg as { n: number }).n}`);
        await sleep(20);
        timeline.push(`b-end:${(msg as { n: number }).n}`);
      }) as ActorHandler,
    });

    // Fire messages quickly
    system.send("actor-a", { n: 1 });
    system.send("actor-b", { n: 1 });

    await sleep(50);

    // Both actors should have started before either finished
    const aStart1 = timeline.indexOf("a-start:1");
    const bStart1 = timeline.indexOf("b-start:1");
    const aEnd1 = timeline.indexOf("a-end:1");
    const bEnd1 = timeline.indexOf("b-end:1");

    // Both started before the latter finished
    expect(aStart1).toBeLessThan(bEnd1);
    expect(bStart1).toBeLessThan(aEnd1);
  });

  it("same actor processes messages sequentially", async () => {
    const system = new ActorSystem();
    const timeline: string[] = [];

    system.spawn({
      id: "serial",
      handler: (async (msg, _ctx, _meta) => {
        timeline.push(`start:${(msg as { n: number }).n}`);
        await sleep(15);
        timeline.push(`end:${(msg as { n: number }).n}`);
      }) as ActorHandler,
    });

    system.send("serial", { n: 1 });
    system.send("serial", { n: 2 });

    await sleep(60);

    // Message 1 must end before message 2 starts
    const m1End = timeline.indexOf("end:1");
    const m2Start = timeline.indexOf("start:2");
    expect(m1End).toBeLessThan(m2Start);
  });

  // ---- Context ----

  it("ctx.self contains the actor's own ref", async () => {
    const system = new ActorSystem();
    let capturedRef: ActorRef | undefined;

    system.spawn({
      id: "self-check",
      handler: (async (_msg, ctx, meta) => {
        capturedRef = ctx.self;
        ctx.reply(meta, "ok");
      }) as ActorHandler,
    });

    await system.ask("self-check", { type: "whoami" });
    expect(capturedRef?.id).toBe("self-check");
  });

  it("ctx.now() returns a timestamp", async () => {
    const system = new ActorSystem();
    let capturedTime = 0;

    system.spawn({
      id: "time-check",
      handler: (async (_msg, ctx, meta) => {
        capturedTime = ctx.now();
        ctx.reply(meta, capturedTime);
      }) as ActorHandler,
    });

    const before = Date.now();
    const _result = await system.ask("time-check", { type: "whattime" });
    const after = Date.now();

    expect(capturedTime).toBeGreaterThanOrEqual(before);
    expect(capturedTime).toBeLessThanOrEqual(after);
  });

  // ---- getActorIds ----

  it("getActorIds returns all spawned actor ids", () => {
    const system = new ActorSystem();
    system.spawn({ id: "a", handler: echoHandler() as ActorHandler });
    system.spawn({ id: "b", handler: echoHandler() as ActorHandler });

    const ids = system.getActorIds();
    expect(ids).toContain("a");
    expect(ids).toContain("b");
    expect(ids.length).toBe(2);
  });
});
