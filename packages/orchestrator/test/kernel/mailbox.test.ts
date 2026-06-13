// ---- Kernel: Mailbox tests ----

import { describe, expect, it } from "bun:test";
import type { Envelope } from "../../src/kernel/envelope.js";
import { MailboxFullError } from "../../src/kernel/errors.js";
import { Mailbox } from "../../src/kernel/mailbox.js";

function makeEnvelope(id: string, payload: unknown): Envelope {
  return {
    id,
    to: "test-actor",
    payload,
    createdAt: Date.now(),
  };
}

describe("Mailbox", () => {
  it("enqueues and processes messages in FIFO order", async () => {
    const processed: string[] = [];
    const mailbox = new Mailbox<string>("test-actor", { capacity: 10 });

    mailbox.setHandler(async (env) => {
      processed.push(env.payload as string);
    });

    mailbox.enqueue(makeEnvelope("e1", "first"));
    mailbox.enqueue(makeEnvelope("e2", "second"));
    mailbox.enqueue(makeEnvelope("e3", "third"));

    // Wait for async processing
    await new Promise((r) => setTimeout(r, 10));

    expect(processed).toEqual(["first", "second", "third"]);
  });

  it("processes only one message at a time per actor", async () => {
    const timeline: string[] = [];
    const mailbox = new Mailbox<string>("test-actor", { capacity: 10 });

    mailbox.setHandler(async (env) => {
      timeline.push(`start:${env.payload}`);
      await new Promise((r) => setTimeout(r, 20));
      timeline.push(`end:${env.payload}`);
    });

    mailbox.enqueue(makeEnvelope("e1", "A"));
    mailbox.enqueue(makeEnvelope("e2", "B"));

    // Wait
    await new Promise((r) => setTimeout(r, 60));

    // Handler for A must complete before B starts
    const aStart = timeline.indexOf("start:A");
    const aEnd = timeline.indexOf("end:A");
    const bStart = timeline.indexOf("start:B");

    expect(aStart).toBeLessThan(aEnd);
    expect(aEnd).toBeLessThan(bStart);
  });

  it("throws MailboxFullError when capacity is exceeded", () => {
    const mailbox = new Mailbox("test-actor", { capacity: 2 });

    mailbox.setHandler(async () => {
      // slow handler so queue builds up
      await new Promise((r) => setTimeout(r, 100));
    });

    // e1 is consumed immediately (processing starts), queue is empty
    mailbox.enqueue(makeEnvelope("e1", "A"));
    // e2 goes to queue (1 in queue)
    mailbox.enqueue(makeEnvelope("e2", "B"));
    // e3 goes to queue (2 in queue = capacity full)
    mailbox.enqueue(makeEnvelope("e3", "C"));
    // e4 should throw because queue is full
    expect(() => mailbox.enqueue(makeEnvelope("e4", "D"))).toThrow(MailboxFullError);
  });

  it("reports isFull correctly", () => {
    const mailbox = new Mailbox("test-actor", { capacity: 2 });
    mailbox.setHandler(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(mailbox.isFull()).toBe(false);
    // e1 consumed immediately
    mailbox.enqueue(makeEnvelope("e1", "A"));
    expect(mailbox.isFull()).toBe(false);
    // e2 goes to queue (1 in queue)
    mailbox.enqueue(makeEnvelope("e2", "B"));
    expect(mailbox.isFull()).toBe(false);
    // e3 goes to queue (2 in queue = full)
    mailbox.enqueue(makeEnvelope("e3", "C"));
    expect(mailbox.isFull()).toBe(true);
  });

  it("after stop, rejects new messages and clears queue", () => {
    const mailbox = new Mailbox("test-actor", { capacity: 10 });
    const processed: string[] = [];

    mailbox.setHandler(async (env) => {
      processed.push(env.payload as string);
    });

    mailbox.enqueue(makeEnvelope("e1", "A"));
    mailbox.stop();

    // After stop, enqueue is a no-op
    mailbox.enqueue(makeEnvelope("e2", "B"));

    expect(mailbox.isStopped).toBe(true);
    expect(mailbox.length).toBe(0);
  });

  it("continues processing after handler throws", async () => {
    const processed: string[] = [];
    const mailbox = new Mailbox<string>("test-actor", { capacity: 10 });

    mailbox.setHandler(async (env) => {
      if (env.payload === "fail") {
        throw new Error("boom");
      }
      processed.push(env.payload as string);
    });

    mailbox.enqueue(makeEnvelope("e1", "fail"));
    mailbox.enqueue(makeEnvelope("e2", "ok"));

    await new Promise((r) => setTimeout(r, 10));

    // "ok" should still be processed after "fail" throws
    expect(processed).toContain("ok");
  });

  it("length reflects current queue size", async () => {
    const mailbox = new Mailbox("test-actor", { capacity: 10 });
    let resolve: () => void;
    const blocker = new Promise<void>((r) => {
      resolve = r;
    });

    mailbox.setHandler(async () => {
      await blocker;
    });

    mailbox.enqueue(makeEnvelope("e1", "A"));
    mailbox.enqueue(makeEnvelope("e2", "B"));
    mailbox.enqueue(makeEnvelope("e3", "C"));

    // One is being processed, two in queue
    expect(mailbox.length).toBe(2);

    resolve!();
    await new Promise((r) => setTimeout(r, 10));

    expect(mailbox.length).toBe(0);
  });
});
