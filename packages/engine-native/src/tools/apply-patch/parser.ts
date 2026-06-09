/**
 * apply_patch tool — structured file patching.
 *
 * Grammar (Codex-compatible):
 *
 * *** Begin Patch
 * *** Add File: path
 * +content
 * *** Update File: path
 * @@
 * -old
 * +new
 * *** Delete File: path
 * *** End Patch
 */

// ---- Types ----

export interface ApplyPatchArgs {
  patch: string;
}

export interface ApplyPatchResult {
  applied: boolean;
  filesAdded: string[];
  filesUpdated: string[];
  filesDeleted: string[];
  filesMoved: Array<{ from: string; to: string }>;
  hunksApplied: number;
}

// ---- Patch operation ----

export type PatchOperation =
  | { kind: "add"; file: string; content: string }
  | { kind: "update"; file: string; hunks: Hunk[] }
  | { kind: "delete"; file: string };

export interface Hunk {
  oldLines: string[];
  newLines: string[];
}

// ---- Parser ----

interface ParseError {
  message: string;
  line?: number;
}

class PatchParseError extends Error {
  constructor(err: ParseError) {
    super(err.line ? `${err.message} (line ${err.line})` : err.message);
    this.name = "PatchParseError";
  }
}

function parsePatch(input: string): { operations: PatchOperation[]; fileSet: Set<string> } {
  const lines = input.split("\n");
  const operations: PatchOperation[] = [];
  const fileSet = new Set<string>();
  let i = 0;

  function line(): string {
    return lines[i] ?? "";
  }

  function advance(): string {
    const l = line();
    i++;
    return l;
  }

  function _peek(): string {
    return lines[i] ?? "";
  }

  function expectLine(expected: string): void {
    const actual = line().trim();
    i++;
    if (actual !== expected) {
      throw new PatchParseError({
        message: `Expected "${expected}" but found "${actual}"`,
        line: i,
      });
    }
  }

  // Begin
  expectLine("*** Begin Patch");

  while (i < lines.length) {
    const trimmed = line().trim();
    if (trimmed === "*** End Patch") {
      i++;
      break;
    }

    if (trimmed.startsWith("*** Add File:")) {
      const file = trimmed.slice("*** Add File:".length).trim();
      if (!file) throw new PatchParseError({ message: "Add File missing path", line: i + 1 });
      validatePath(file);
      advance();

      // Read content lines (prefixed with +)
      let content = "";
      while (i < lines.length) {
        const l = line();
        if (l.startsWith("+")) {
          content += (content ? "\n" : "") + l.slice(1);
          advance();
        } else if (l.trim() === "" || l.startsWith("*** ") || l.trim() === "*** End Patch") {
          break;
        } else {
          throw new PatchParseError({
            message: `Expected "+" prefix for Add File content, got "${l}"`,
            line: i + 1,
          });
        }
      }

      operations.push({ kind: "add", file, content });
      fileSet.add(file);
      continue;
    }

    if (trimmed.startsWith("*** Update File:")) {
      const file = trimmed.slice("*** Update File:".length).trim();
      if (!file) throw new PatchParseError({ message: "Update File missing path", line: i + 1 });
      validatePath(file);
      advance();

      // Hunks
      const hunks: Hunk[] = [];
      while (i < lines.length) {
        const l = line().trim();
        if (l.startsWith("*** ") || l === "*** End Patch") break;

        if (l === "@@") {
          advance();
          const oldLines: string[] = [];
          const newLines: string[] = [];

          let inOld = true;
          while (i < lines.length) {
            const hl = line();
            if (hl.startsWith("-")) {
              if (!inOld) {
                // Transition from + back to - is not allowed in a single hunk
                // Treat as next hunk marker
                break;
              }
              oldLines.push(hl.slice(1));
              advance();
            } else if (hl.startsWith("+")) {
              inOld = false;
              newLines.push(hl.slice(1));
              advance();
            } else if (
              hl.trim() === "@@" ||
              hl.trim().startsWith("*** ") ||
              hl.trim() === "*** End Patch"
            ) {
              break;
            } else if (hl.trim() === "") {
              advance();
            } else {
              throw new PatchParseError({
                message: `Expected "-" or "+" prefix in hunk, got "${hl}"`,
                line: i + 1,
              });
            }
          }

          if (oldLines.length === 0 && newLines.length === 0) {
            throw new PatchParseError({ message: "Empty hunk in Update File", line: i + 1 });
          }

          hunks.push({ oldLines, newLines });
        } else if (l === "") {
          advance();
        } else {
          throw new PatchParseError({
            message: `Expected "@@" for hunk start, got "${l}"`,
            line: i + 1,
          });
        }
      }

      if (hunks.length === 0) {
        throw new PatchParseError({ message: "Update File has no hunks", line: i + 1 });
      }

      operations.push({ kind: "update", file, hunks });
      fileSet.add(file);
      continue;
    }

    if (trimmed.startsWith("*** Delete File:")) {
      const file = trimmed.slice("*** Delete File:".length).trim();
      if (!file) throw new PatchParseError({ message: "Delete File missing path", line: i + 1 });
      validatePath(file);
      advance();

      operations.push({ kind: "delete", file });
      fileSet.add(file);
      continue;
    }

    if (trimmed === "" || trimmed.startsWith("*** ")) {
      advance();
      continue;
    }

    throw new PatchParseError({
      message: `Unexpected line: "${trimmed}"`,
      line: i + 1,
    });
  }

  return { operations, fileSet };
}

/** Validate a file path is safe. */
function validatePath(path: string): void {
  if (!path || path.trim() === "") {
    throw new PatchParseError({ message: "Empty file path" });
  }
  if (path.startsWith("/")) {
    throw new PatchParseError({ message: `Absolute paths are not allowed: ${path}` });
  }
  if (path.includes("..")) {
    throw new PatchParseError({ message: `Path traversal not allowed: ${path}` });
  }
  if (/[<>:"|?*]/.test(path)) {
    throw new PatchParseError({ message: `Invalid characters in path: ${path}` });
  }
}

export type { ParseError };
export { PatchParseError, parsePatch };
