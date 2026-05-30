import * as fs from "node:fs/promises";
import { resolvePathFromCwd } from "./utils.js";

async function readTextFile(absolutePath: string): Promise<string> {
  return fs.readFile(absolutePath, "utf-8");
}

export async function readTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  if (!path) throw new Error("read requires a string path");
  const offset = typeof args.offset === "number" ? Math.max(1, Math.floor(args.offset)) : 1;
  const limit = typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : undefined;
  const absolutePath = resolvePathFromCwd(cwd, path);
  const raw = await readTextFile(absolutePath);
  const lines = raw.split("\n");
  const selected = lines.slice(offset - 1, limit ? offset - 1 + limit : undefined);
  return {
    path,
    absolutePath,
    offset,
    limit: limit ?? null,
    lineCount: selected.length,
    content: selected.join("\n"),
  };
}
