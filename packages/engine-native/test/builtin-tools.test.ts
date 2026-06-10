import { describe, expect, it } from "bun:test";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createBuiltinCodingToolSet } from "piko-engine-native";

describe("builtin coding tools", () => {
  it("supports shell and apply_patch", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-tools-"));
    const tools = createBuiltinCodingToolSet(cwd);

    // shell tool
    const shellResult = await tools.registry.shell({
      command: "printf 'hello from shell'",
    });
    expect(shellResult).toMatchObject({
      exitCode: 0,
      stdout: "hello from shell",
    });

    // apply_patch: add file
    const addResult = await tools.registry.apply_patch({
      patch: `*** Begin Patch
*** Add File: hello.txt
+hello, piko!
*** End Patch`,
    });
    expect(addResult).toMatchObject({
      applied: true,
      filesAdded: ["hello.txt"],
    });

    // apply_patch: update file
    const updateResult = await tools.registry.apply_patch({
      patch: `*** Begin Patch
*** Update File: hello.txt
@@
-hello, piko!
+hello, world!
*** End Patch`,
    });
    expect(updateResult).toMatchObject({
      applied: true,
      filesUpdated: ["hello.txt"],
      hunksApplied: 1,
    });

    // apply_patch: delete file
    const deleteResult = await tools.registry.apply_patch({
      patch: `*** Begin Patch
*** Delete File: hello.txt
*** End Patch`,
    });
    expect(deleteResult).toMatchObject({
      applied: true,
      filesDeleted: ["hello.txt"],
    });
  });

  it("shell tool handles errors", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-shell-"));
    const tools = createBuiltinCodingToolSet(cwd);

    const result = await tools.registry.shell({ command: "exit 1" });
    expect(result).toMatchObject({ exitCode: 1 });
  });
});
