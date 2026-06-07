import { describe, expect, it } from "bun:test";
import { buildNativeSystemPrompt } from "piko-engine-native";

describe("buildNativeSystemPrompt", () => {
  it("includes tools and workspace context", () => {
    const prompt = buildNativeSystemPrompt({
      cwd: "/tmp/piko-project",
      tools: [
        { name: "read", description: "Read file contents" },
        { name: "grep", description: "Search file contents by pattern" },
        { name: "find", description: "Find files by glob pattern" },
        { name: "ls", description: "List directory contents" },
      ],
    });

    expect(prompt).toContain("read: Read file contents");
    expect(prompt).toContain("grep: Search file contents by pattern");
    expect(prompt).toContain("find: Find files by glob pattern");
    expect(prompt).toContain("ls: List directory contents");
    expect(prompt).toContain("Current working directory: /tmp/piko-project");
  });
});
