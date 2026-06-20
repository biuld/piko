import { describe, expect, it } from "bun:test";
import { raceWithAbort } from "../../src/actors/agent/tool-executor.js";

function rejectAfter(ms: number): Promise<never> {
  return new Promise((_, reject) => setTimeout(() => reject(new Error("test timed out")), ms));
}

describe("raceWithAbort", () => {
  it("stops awaiting a provider that ignores AbortSignal", async () => {
    const controller = new AbortController();
    const providerNeverSettles = new Promise<string>(() => {});
    const result = raceWithAbort(providerNeverSettles, controller.signal);

    controller.abort();

    const error = await Promise.race([result.catch((reason) => reason), rejectAfter(250)]);
    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe("AbortError");
  });

  it("preserves normal provider completion", async () => {
    const controller = new AbortController();
    await expect(raceWithAbort(Promise.resolve("done"), controller.signal)).resolves.toBe("done");
  });

  it("observes a provider rejection that arrives after abort", async () => {
    const controller = new AbortController();
    let rejectProvider!: (error: Error) => void;
    const provider = new Promise<string>((_, reject) => {
      rejectProvider = reject;
    });

    const result = raceWithAbort(provider, controller.signal);
    controller.abort();
    await expect(result).rejects.toMatchObject({ name: "AbortError" });

    // This late rejection is consumed by raceWithAbort's attached handler.
    rejectProvider(new Error("late provider failure"));
    await Promise.resolve();
  });
});
