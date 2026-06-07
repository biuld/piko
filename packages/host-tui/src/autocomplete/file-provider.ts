// ============================================================================
// FileAutocompleteProvider — provides file/path completions for @ mentions
// ============================================================================

import * as fs from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import type { AutocompleteItem, AutocompleteProvider, AutocompleteSuggestions } from "./types.js";

export class FileAutocompleteProvider implements AutocompleteProvider {
  private cwd: string;

  constructor(cwd: string) {
    this.cwd = cwd;
  }

  async getSuggestions(
    input: string,
    cursor: number,
    _options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null> {
    const textBeforeCursor = input.slice(0, cursor);
    const atIdx = textBeforeCursor.lastIndexOf("@");
    if (atIdx < 0) return null;

    const pathFragment = textBeforeCursor.slice(atIdx + 1);
    // Don't trigger mid-word (e.g., email addresses)
    if (pathFragment.includes(" ")) return null;

    const prefix = textBeforeCursor.slice(atIdx); // e.g., "@src/uti"

    try {
      // Determine the directory to list and the filename prefix to match
      const fullPath = resolve(this.cwd, pathFragment || ".");
      const searchDir = pathFragment ? dirname(fullPath) : this.cwd;
      const namePrefix = pathFragment ? basename(fullPath) : "";

      const entries = await fs.readdir(searchDir, { withFileTypes: true });
      const matches = entries
        .filter((e) => e.name.startsWith(namePrefix))
        .filter((e) => !e.name.startsWith("."))
        .slice(0, 20);

      if (matches.length === 0) return null;

      // Build the replacement value: @ + dir/ + filename
      const dirPart = pathFragment.includes("/")
        ? pathFragment.slice(0, pathFragment.lastIndexOf("/") + 1)
        : "";

      const items: AutocompleteItem[] = matches.map((e) => ({
        value: `@${dirPart}${e.name}${e.isDirectory() ? "/" : ""}`,
        label: e.name + (e.isDirectory() ? "/" : ""),
        providerId: "file",
        description: e.isDirectory() ? "directory" : "file",
      }));

      return { prefix, providerId: "file", items };
    } catch {
      return null;
    }
  }

  applyCompletion(
    input: string,
    cursor: number,
    item: AutocompleteItem,
    _prefix: string,
  ): { input: string; cursor: number } {
    const before = input.slice(0, cursor);
    const atIdx = before.lastIndexOf("@");
    if (atIdx < 0) return { input, cursor };

    const newBefore = before.slice(0, atIdx) + item.value;
    const newInput = newBefore + input.slice(cursor);
    return { input: newInput, cursor: newBefore.length };
  }
}
