import { describe, expect, it } from "vitest";
import type { SessionMeta } from "piko-host-runtime";
import { createThreadedSessionSelectItems, formatSessionTreeLines } from "../src/session-tree.js";

function buildSession(overrides: Partial<SessionMeta> & Pick<SessionMeta, "id" | "path" | "cwd">): SessionMeta {
  return {
    id: overrides.id,
    path: overrides.path,
    cwd: overrides.cwd,
    created: overrides.created ?? "2026-01-01T00:00:00.000Z",
    modified: overrides.modified ?? "2026-01-01T00:00:00.000Z",
    model: overrides.model ?? "test-model",
    messageCount: overrides.messageCount ?? 1,
    preview: overrides.preview ?? "",
    parentSessionPath: overrides.parentSessionPath,
    name: overrides.name,
  };
}

describe("session-tree helpers", () => {
  it("formats threaded session lines and selector items from parent relationships", () => {
    const root = buildSession({
      id: "root",
      path: "/tmp/root.jsonl",
      cwd: "/tmp/a",
      name: "Root",
      modified: "2026-01-03T00:00:00.000Z",
    });
    const child = buildSession({
      id: "child",
      path: "/tmp/child.jsonl",
      cwd: "/tmp/a",
      name: "Child",
      parentSessionPath: root.path,
      modified: "2026-01-02T00:00:00.000Z",
    });
    const sibling = buildSession({
      id: "sibling",
      path: "/tmp/sibling.jsonl",
      cwd: "/tmp/b",
      name: "Sibling",
      modified: "2026-01-01T00:00:00.000Z",
    });

    const lines = formatSessionTreeLines([child, sibling, root]);
    expect(lines[0]).toContain("Root");
    expect(lines[1]).toContain("└─ Child");
    expect(lines[2]).toContain("Sibling");

    const items = createThreadedSessionSelectItems([child, sibling, root]);
    expect(items[0]?.label).toBe("Root");
    expect(items[1]?.label).toContain("└─ Child");
    expect(items[2]?.label).toBe("Sibling");
  });
});
