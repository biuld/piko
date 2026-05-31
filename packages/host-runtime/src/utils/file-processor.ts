/**
 * File argument processor — handles @path arguments in user messages.
 *
 * Supports:
 * - @path — reads file content and appends to prompt
 * - Glob expansion (via @pattern syntax — TBD)
 *
 * Used by CLI (non-interactive mode) and TUI (editor submit handler).
 */

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

export interface FileArgument {
  path: string;
  content: string;
}

/**
 * Process @file references in a prompt string.
 * Replaces @path with XML-wrapped file content.
 */
export function processFileArguments(
  text: string,
  cwd: string = process.cwd(),
): { expanded: string; files: FileArgument[] } {
  const files: FileArgument[] = [];
  let expanded = text;

  // Match @path patterns
  const pattern = /@([^\s]+)/g;
  let match;

  while ((match = pattern.exec(text)) !== null) {
    const rawPath = match[1];
    const resolvedPath = resolve(cwd, rawPath);

    if (!existsSync(resolvedPath)) continue;

    try {
      const content = readFileSync(resolvedPath, "utf-8");
      // Skip binary files
      if (content.includes("\0")) continue;

      files.push({ path: resolvedPath, content });

      // Replace @path with XML-wrapped file content
      const fileTag = `<file path="${resolvedPath}">\n${content}\n</file>`;
      expanded = expanded.replace(match[0], fileTag);
    } catch {
      // Can't read — leave @path as-is
    }
  }

  return { expanded, files };
}
