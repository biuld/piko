import { describe, expect, it } from "bun:test";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createBuiltinCodingToolSet, createLegacyFileToolSet } from "piko-engine-native";

describe("builtin coding tools (new: shell + apply_patch)", () => {
  it("supports shell and apply_patch", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-new-tools-"));
    const tools = createBuiltinCodingToolSet(cwd);

    // shell tool
    const shellResult = await tools.registry.shell({
      command: "printf 'hello from shell'",
    });
    expect(shellResult).toMatchObject({
      exitCode: 0,
      stdout: "hello from shell",
      timedOut: false,
    });

    // apply_patch tool: add a file
    const addPatch = `*** Begin Patch
*** Add File: hello.txt
+hello, piko!
*** End Patch`;
    const addResult = await tools.registry.apply_patch({
      patch: addPatch,
    });
    expect(addResult).toMatchObject({
      applied: true,
      filesAdded: ["hello.txt"],
      hunksApplied: 0,
    });

    // apply_patch tool: update a file
    const updatePatch = `*** Begin Patch
*** Update File: hello.txt
@@
-hello, piko!
+hello, world!
*** End Patch`;
    const updateResult = await tools.registry.apply_patch({
      patch: updatePatch,
    });
    expect(updateResult).toMatchObject({
      applied: true,
      filesUpdated: ["hello.txt"],
      hunksApplied: 1,
    });

    // apply_patch tool: delete a file
    const deletePatch = `*** Begin Patch
*** Delete File: hello.txt
*** End Patch`;
    const deleteResult = await tools.registry.apply_patch({
      patch: deletePatch,
    });
    expect(deleteResult).toMatchObject({
      applied: true,
      filesDeleted: ["hello.txt"],
    });
  });

  it("shell tool handles errors and timeouts", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-shell-errors-"));
    const tools = createBuiltinCodingToolSet(cwd);

    const badResult = await tools.registry.shell({
      command: "exit 1",
    });
    expect(badResult).toMatchObject({
      exitCode: 1,
    });
  });
});

describe("legacy coding tools", () => {
  it("supports write, read, edit, bash, grep, find, and ls", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-builtin-tools-"));
    const tools = createLegacyFileToolSet(cwd);

    await fs.mkdir(join(cwd, "src"), { recursive: true });
    await fs.mkdir(join(cwd, "nested", "deeper"), { recursive: true });

    const writeResult = await tools.registry.write({
      path: "note.txt",
      content: "hello world",
    });
    expect(writeResult).toMatchObject({
      path: "note.txt",
      written: true,
    });

    const readResult = await tools.registry.read({
      path: "note.txt",
    });
    expect(readResult).toMatchObject({
      path: "note.txt",
      content: "hello world",
    });

    const editResult = await tools.registry.edit({
      path: "note.txt",
      edits: [{ oldText: "world", newText: "piko" }],
    });
    expect(editResult).toMatchObject({
      path: "note.txt",
      patched: true,
      editsApplied: 1,
    });

    const reread = await tools.registry.read({
      path: "note.txt",
    });
    expect(reread).toMatchObject({
      content: "hello piko",
    });

    const bashResult = await tools.registry.bash({
      command: "printf 'ok'",
    });
    expect(bashResult).toMatchObject({
      command: "printf 'ok'",
      stdout: "ok",
      exitCode: 0,
    });

    await fs.writeFile(
      join(cwd, "src", "main.ts"),
      "export const hello = 'piko';\nconsole.log(hello);\n",
    );
    await fs.writeFile(
      join(cwd, "nested", "deeper", "worker.ts"),
      "export const worker = 'hello piko';\n",
    );

    const grepResult = await tools.registry.grep({
      pattern: "hello",
      path: "src",
      glob: "*.ts",
    });
    expect(grepResult).toBeTypeOf("string");
    expect(String(grepResult)).toContain("src");
    expect(String(grepResult)).toContain("hello");

    const findResult = await tools.registry.find({
      pattern: "**/*.ts",
      path: ".",
    });
    expect(findResult).toBeTypeOf("string");
    expect(String(findResult)).toContain("src/main.ts");
    expect(String(findResult)).toContain("nested/deeper/worker.ts");

    const lsResult = await tools.registry.ls({
      path: ".",
    });
    expect(lsResult).toBeTypeOf("string");
    expect(String(lsResult)).toContain("src/");
    expect(String(lsResult)).toContain("note.txt");
  });
});
