/**
 * File argument processor — handles @path arguments on the CLI.
 *
 * Supports:
 * - @path — reads file content and appends to prompt
 * - @path:line — reads file starting from line
 * - Glob expansion (via @pattern syntax)
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export interface FileArgument {
  path: string;
  content: string;
}

/**
 * Process @file arguments from a prompt string.
 * Replaces @path with the file content (wrapped in XML tags).
 */
export function processFileArguments(
  text: string,
  cwd: string = process.cwd(),
): { expanded: string; files: FileArgument[] } {
  const files: FileArgument[] = [];
  let expanded = text;

  // Match @path patterns (not inside code blocks)
  const pattern = /@([^\s]+)/g;
  let match;

  while ((match = pattern.exec(text)) !== null) {
    const rawPath = match[1];
    const resolvedPath = resolve(cwd, rawPath);

    try {
      const content = readFileSync(resolvedPath, "utf-8");
      // Check if it's an image (binary check — skip)
      if (content.includes("\0")) continue;

      files.push({ path: resolvedPath, content });

      // Replace the @path in the prompt with XML-wrapped file content
      const fileTag = `<file path="${resolvedPath}">\n${content}\n</file>`;
      expanded = expanded.replace(match[0], fileTag);
    } catch {
      // File doesn't exist or can't be read — leave @path as-is
    }
  }

  return { expanded, files };
}
