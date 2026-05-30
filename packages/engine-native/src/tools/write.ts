import * as fs from "node:fs/promises";
import { dirname } from "node:path";
import { resolvePathFromCwd } from "./utils.js";

export async function writeTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  const content = typeof args.content === "string" ? args.content : undefined;
  if (!path) throw new Error("write requires a string path");
  if (content === undefined) throw new Error("write requires string content");
  const absolutePath = resolvePathFromCwd(cwd, path);
  await fs.mkdir(dirname(absolutePath), { recursive: true });
  await fs.writeFile(absolutePath, content, "utf-8");
  return { path, absolutePath, bytesWritten: Buffer.byteLength(content, "utf-8"), written: true };
}
