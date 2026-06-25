import { afterEach, describe, expect, test } from "bun:test";
import {
  type DebugTraceRecord,
  debugTrace,
  setDebugTraceSink,
  startDebugSpan,
} from "../src/index.js";

afterEach(() => setDebugTraceSink(undefined));

describe("debug trace", () => {
  test("emits structured lifecycle records without requiring a sink", () => {
    debugTrace({ stage: "disabled" });

    const records: DebugTraceRecord[] = [];
    setDebugTraceSink((record) => records.push(record));
    const span = startDebugSpan("tool.execute", { taskId: "task-1", toolCallId: "call-1" });
    span.end({ outcome: "completed" });

    expect(records).toHaveLength(2);
    expect(records[0]?.stage).toBe("tool.execute");
    expect(records[1]).toMatchObject({
      stage: "tool.execute.end",
      taskId: "task-1",
      toolCallId: "call-1",
      outcome: "completed",
    });
    expect(records[1]?.durationMs).toBeGreaterThanOrEqual(0);
  });

  test("ignores sink failures", () => {
    setDebugTraceSink(() => {
      throw new Error("disk unavailable");
    });
    expect(() => debugTrace({ stage: "safe" })).not.toThrow();
  });
});
