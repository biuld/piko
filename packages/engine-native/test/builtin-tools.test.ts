import { describe, expect, it } from "bun:test";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createBuiltinCodingToolSet } from "piko-engine-native";

describe("builtin coding tools", () => {
  it("supports write, read, edit, bash, grep, find, and ls", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-builtin-tools-"));
    const tools = createBuiltinCodingToolSet(cwd);

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
