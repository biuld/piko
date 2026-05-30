import * as fs from "node:fs/promises";
import {
  createToolTextOutput,
  DEFAULT_LS_LIMIT,
  resolvePathFromCwd,
  truncateLines,
} from "./utils.js";

export async function lsTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const targetPath = typeof args.path === "string" && args.path.trim() ? args.path : ".";
  const limit =
    typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : DEFAULT_LS_LIMIT;
  const absolutePath = resolvePathFromCwd(cwd, targetPath);
  const stats = await fs.stat(absolutePath);
  if (!stats.isDirectory()) throw new Error(`ls requires a directory path: ${targetPath}`);

  const dirEntries = await fs.readdir(absolutePath, { withFileTypes: true });
  dirEntries.sort((left, right) => left.name.localeCompare(right.name));
  const formattedEntries = dirEntries.map((entry) =>
    entry.isDirectory() ? `${entry.name}/` : entry.name,
  );
  const truncated = truncateLines(formattedEntries, limit);

  return createToolTextOutput(
    `ls ${targetPath}`,
    truncated.lines,
    truncated.truncated ? `Truncated to ${limit} entries` : undefined,
  );
}
