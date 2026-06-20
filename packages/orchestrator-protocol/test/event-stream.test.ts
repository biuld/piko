import { describe, expect, it } from "bun:test";
import { EventStream } from "../src/index.js";

describe("EventStream", () => {
  it("end() is idempotent and does not change the first result", async () => {
    const stream = new EventStream<string, number>();

    stream.end(42);
    stream.end(100); // subsequent calls should be ignored

    const result = await stream.result();
    expect(result).toBe(42);
  });

  it("end() wakes up a waiting async iterator", async () => {
    const stream = new EventStream<string, number>();

    const iterator = stream[Symbol.asyncIterator]();

    const nextPromise = iterator.next();

    stream.end(42);

    const nextResult = await nextPromise;
    expect(nextResult.done).toBe(true);

    const result = await stream.result();
    expect(result).toBe(42);
  });
});
