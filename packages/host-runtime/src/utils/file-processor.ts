/**
 * File argument processor — handles @path arguments in user messages.
 *
 * Supports:
 * - @path — reads text file content and wraps in XML
 * - @path for images — converts to base64 data URI
 *
 * Used by CLI (non-interactive mode) and TUI (editor submit handler).
 */

import { existsSync, readFileSync } from "node:fs";
import { extname, resolve } from "node:path";

const IMAGE_EXTENSIONS = new Set([".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]);

export interface FileArgument {
  path: string;
  content: string;
  /** Set when the file is an image. */
  isImage?: boolean;
}

/**
 * Process @file references in a prompt string.
 * For text files: replaces @path with XML-wrapped content.
 * For images: replaces @path with a data URI marker.
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

    const ext = extname(resolvedPath).toLowerCase();
    const isImageFile = IMAGE_EXTENSIONS.has(ext);

    try {
      if (isImageFile) {
        // Read as binary and convert to base64 data URI
        const buf = readFileSync(resolvedPath);
        const mimeMap: Record<string, string> = {
          ".png": "image/png",
          ".jpg": "image/jpeg",
          ".jpeg": "image/jpeg",
          ".gif": "image/gif",
          ".webp": "image/webp",
          ".bmp": "image/bmp",
        };
        const mimeType = mimeMap[ext] ?? "image/png";
        const dataUri = `data:${mimeType};base64,${buf.toString("base64")}`;

        files.push({ path: resolvedPath, content: dataUri, isImage: true });
        expanded = expanded.replace(match[0], `@image:${resolvedPath}`);
      } else {
        // Text file
        const content = readFileSync(resolvedPath, "utf-8");
        // Skip binary files
        if (content.includes("\0")) continue;

        files.push({ path: resolvedPath, content });

        // Replace @path with XML-wrapped file content
        const fileTag = `<file path="${resolvedPath}">\n${content}\n</file>`;
        expanded = expanded.replace(match[0], fileTag);
      }
    } catch {
      // Can't read — leave @path as-is
    }
  }

  return { expanded, files };
}
