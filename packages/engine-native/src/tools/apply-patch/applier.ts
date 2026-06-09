import * as fs from "node:fs/promises";
import { dirname, resolve } from "node:path";
import type { Hunk, PatchOperation } from "./parser.js";

export interface ApplyResult {
  filesAdded: string[];
  filesUpdated: string[];
  filesDeleted: string[];
  hunksApplied: number;
  errors: string[];
}

/**
 * Apply parsed patch operations to the workspace.
 */
export async function applyOperations(
  cwd: string,
  operations: PatchOperation[],
): Promise<ApplyResult> {
  const result: ApplyResult = {
    filesAdded: [],
    filesUpdated: [],
    filesDeleted: [],
    hunksApplied: 0,
    errors: [],
  };

  for (const op of operations) {
    try {
      switch (op.kind) {
        case "add":
          await applyAdd(cwd, op.file, op.content);
          result.filesAdded.push(op.file);
          break;
        case "update":
          await applyUpdate(cwd, op.file, op.hunks);
          result.filesUpdated.push(op.file);
          result.hunksApplied += op.hunks.length;
          break;
        case "delete":
          await applyDelete(cwd, op.file);
          result.filesDeleted.push(op.file);
          break;
      }
    } catch (err) {
      result.errors.push(
        `${op.kind} ${op.file}: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  return result;
}

async function applyAdd(cwd: string, file: string, content: string): Promise<void> {
  const absolutePath = resolve(cwd, file);

  // Ensure within workspace
  if (!absolutePath.startsWith(cwd)) {
    throw new Error(`Path outside workspace: ${file}`);
  }

  // Check if file already exists
  try {
    await fs.stat(absolutePath);
    throw new Error(`File already exists: ${file}`);
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") throw err;
  }

  await fs.mkdir(dirname(absolutePath), { recursive: true });
  await fs.writeFile(absolutePath, content, "utf-8");
}

async function applyUpdate(cwd: string, file: string, hunks: Hunk[]): Promise<void> {
  const absolutePath = resolve(cwd, file);

  if (!absolutePath.startsWith(cwd)) {
    throw new Error(`Path outside workspace: ${file}`);
  }

  let content: string;
  try {
    content = await fs.readFile(absolutePath, "utf-8");
  } catch {
    throw new Error(`File not found: ${file}`);
  }

  const lines = content.split("\n");

  for (const hunk of hunks) {
    // Find and replace oldLines with newLines
    const matchIndex = findHunkMatch(lines, hunk.oldLines);
    if (matchIndex === -1) {
      throw new Error(`Hunk not found in ${file}: expected pattern "${hunk.oldLines.join("\\n")}"`);
    }

    // Apply: remove oldLines, insert newLines
    lines.splice(matchIndex, hunk.oldLines.length, ...hunk.newLines);
  }

  await fs.writeFile(absolutePath, lines.join("\n"), "utf-8");
}

async function applyDelete(cwd: string, file: string): Promise<void> {
  const absolutePath = resolve(cwd, file);

  if (!absolutePath.startsWith(cwd)) {
    throw new Error(`Path outside workspace: ${file}`);
  }

  try {
    await fs.unlink(absolutePath);
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code !== "ENOENT") throw err;
  }
}

/**
 * Find the first occurrence of oldLines in the target lines.
 * Returns the starting line index, or -1 if not found.
 */
function findHunkMatch(haystack: string[], needle: string[]): number {
  if (needle.length === 0) return -1;

  for (let i = 0; i <= haystack.length - needle.length; i++) {
    let match = true;
    for (let j = 0; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) {
        match = false;
        break;
      }
    }
    if (match) return i;
  }
  return -1;
}
