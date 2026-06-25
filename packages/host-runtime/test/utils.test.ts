import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import type { Message } from "piko-orch-protocol";
import { getGitBranch } from "../src/utils/git.js";
import { getTimings, resetTimings, Timings } from "../src/utils/timings.js";
import { computeCumulativeUsage, getContextPercent } from "../src/utils/token-usage.js";
import { execSync, fs, join, tmpdir } from "./bun-test-utils.js";

describe("token-usage utility", () => {
  it("computeCumulativeUsage calculates sum of usage properties", () => {
    const messages: Message[] = [
      { role: "user", content: "hello", timestamp: Date.now() },
      {
        role: "assistant",
        content: "reply 1",
        timestamp: Date.now(),
        usage: {
          input: 10,
          output: 20,
          cacheRead: 5,
          cacheWrite: 2,
          cost: { total: 5 },
        },
      } as any,
      {
        role: "assistant",
        content: "reply 2",
        timestamp: Date.now(),
        usage: {
          input: 15,
          output: 30,
          cacheRead: 3,
          cacheWrite: 4,
          cost: { total: 8 },
        },
      } as any,
    ];

    const usage = computeCumulativeUsage(messages);
    expect(usage).toEqual({
      input: 25,
      output: 50,
      cacheRead: 8,
      cacheWrite: 6,
      cost: 13,
    });
  });

  it("computeCumulativeUsage handles missing or partial usage", () => {
    const messages: Message[] = [{ role: "assistant", content: "reply" } as any];
    const usage = computeCumulativeUsage(messages);
    expect(usage).toEqual({
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      cost: 0,
    });
  });

  it("getContextPercent calculates percentage correctly", () => {
    expect(getContextPercent(10, 100)).toBe(10);
    expect(getContextPercent(0, 100)).toBe(0);
    expect(getContextPercent(100, 0)).toBe(0);
    expect(getContextPercent(100, -5)).toBe(0);
  });
});

describe("timings utility", () => {
  it("ignores timings when disabled", () => {
    const timings = new Timings(false);
    timings.time("test");
    timings.timeEnd();
    expect(timings.getResults()).toEqual([]);
  });

  it("records timing section when enabled", async () => {
    const timings = new Timings(true);
    timings.time("a");
    await new Promise((r) => setTimeout(r, 10));
    timings.timeEnd();

    const results = timings.getResults();
    expect(results).toHaveLength(1);
    expect(results[0].label).toBe("a");
    expect(results[0].elapsedMs).toBeGreaterThan(0);
  });

  it("supports ending timing by label", async () => {
    const timings = new Timings(true);
    timings.time("b");
    timings.timeEndLabel("b");

    const results = timings.getResults();
    expect(results).toHaveLength(1);
    expect(results[0].label).toBe("b");
  });

  it("handles timeEnd with empty stack gracefully", () => {
    const timings = new Timings(true);
    // timeEnd on empty stack
    expect(() => timings.timeEnd()).not.toThrow();
  });

  it("handles timeEndLabel with nonexistent label gracefully", () => {
    const timings = new Timings(true);
    expect(() => timings.timeEndLabel("nonexistent")).not.toThrow();
  });

  it("printTimings returns early if no results exist", () => {
    const timings = new Timings(true);
    const writeSpy = process.stderr.write;
    let output = "";
    process.stderr.write = (str: any) => {
      output += str;
      return true;
    };
    try {
      timings.printTimings();
    } finally {
      process.stderr.write = writeSpy;
    }
    expect(output).toBe("");
  });

  it("telemetry printTimings runs and formats output", () => {
    const timings = new Timings(true);
    timings.time("section");
    timings.timeEnd();

    const writeSpy = process.stderr.write;
    let output = "";
    process.stderr.write = (str: any) => {
      output += str;
      return true;
    };
    try {
      timings.printTimings();
    } finally {
      process.stderr.write = writeSpy;
    }

    expect(output).toContain("Startup Timing");
    expect(output).toContain("section");
    expect(output).toContain("TOTAL");
  });

  it("getTimings and resetTimings manage singleton correctly", () => {
    const initial = getTimings();
    expect(initial).toBeInstanceOf(Timings);
    expect(getTimings()).toBe(initial);

    resetTimings();
    expect(getTimings()).not.toBe(initial);
  });

  it("checks if timings is enabled and respects environment variable", () => {
    const timingsTrue = new Timings(true);
    expect(timingsTrue.isEnabled()).toBe(true);

    const timingsFalse = new Timings(false);
    expect(timingsFalse.isEnabled()).toBe(false);

    // Test environment variable override
    const originalEnv = process.env.PIKO_STARTUP_BENCHMARK;
    process.env.PIKO_STARTUP_BENCHMARK = "1";
    try {
      const timingsEnv = new Timings(false);
      expect(timingsEnv.isEnabled()).toBe(true);
    } finally {
      process.env.PIKO_STARTUP_BENCHMARK = originalEnv;
    }
  });
});

describe("git utility", () => {
  let tempCwd: string;

  beforeEach(() => {
    tempCwd = fs.mkdtempSync(join(tmpdir(), "piko-git-test-"));
  });

  afterEach(() => {
    try {
      fs.rmSync(tempCwd, { recursive: true, force: true });
    } catch {}
  });

  it("returns undefined for non-git repository", () => {
    const branch = getGitBranch(tempCwd);
    expect(branch).toBeUndefined();
  });

  it("returns branch name for initialized git repository", () => {
    execSync("git init", { cwd: tempCwd, stdio: "ignore" });
    execSync("git config user.name 'Test'", { cwd: tempCwd });
    execSync("git config user.email 'test@test.com'", { cwd: tempCwd });
    fs.writeFileSync(join(tempCwd, "temp.txt"), "hello");
    execSync("git add temp.txt && git commit -m 'initial'", { cwd: tempCwd, stdio: "ignore" });

    const branch = getGitBranch(tempCwd);
    expect(branch).toBeDefined();
    expect(branch).toMatch(/^(master|main)$/);
  });
});
