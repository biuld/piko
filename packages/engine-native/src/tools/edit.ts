import * as fs from "node:fs/promises";
import { countOccurrences, resolvePathFromCwd } from "./utils.js";

async function readTextFile(absolutePath: string): Promise<string> {
  return fs.readFile(absolutePath, "utf-8");
}

export async function editTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  if (!path) throw new Error("edit requires a string path");
  const edits = Array.isArray(args.edits) ? args.edits : undefined;
  if (!edits || edits.length === 0) throw new Error("edit requires a non-empty edits array");

  const normalizedEdits = edits.map((edit) => {
    if (!edit || typeof edit !== "object") throw new Error("edit entries must be objects");
    const oldText =
      typeof (edit as { oldText?: unknown }).oldText === "string"
        ? (edit as { oldText: string }).oldText
        : undefined;
    const newText =
      typeof (edit as { newText?: unknown }).newText === "string"
        ? (edit as { newText: string }).newText
        : undefined;
    if (oldText === undefined || newText === undefined) {
      throw new Error("each edit requires oldText and newText strings");
    }
    return { oldText, newText };
  });

  const absolutePath = resolvePathFromCwd(cwd, path);
  let content = await readTextFile(absolutePath);
  for (const edit of normalizedEdits) {
    const matches = countOccurrences(content, edit.oldText);
    if (matches === 0) throw new Error(`edit target not found in ${path}`);
    if (matches > 1) throw new Error(`edit target is ambiguous in ${path}`);
    content = content.replace(edit.oldText, edit.newText);
  }
  await fs.writeFile(absolutePath, content, "utf-8");
  return { path, absolutePath, editsApplied: normalizedEdits.length, patched: true };
}
