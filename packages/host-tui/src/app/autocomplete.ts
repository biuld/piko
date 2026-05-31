import type { AutocompleteProvider, AutocompleteSuggestions } from "@earendil-works/pi-tui";
import type { ExtensionHost } from "../extensions/index.js";
import { COMMANDS } from "../commands/index.js";

export function createAutocomplete(extensionHost?: ExtensionHost): AutocompleteProvider {
  return {
    async getSuggestions(lines: string[], cursorLine: number, cursorCol: number): Promise<AutocompleteSuggestions | null> {
      const line = lines[cursorLine] ?? "";
      const prefix = line.slice(0, cursorCol);
      if (!prefix.startsWith("/")) return null;
      const allCommands = [
        ...COMMANDS,
        ...(extensionHost?.commands.map((c) => ({ value: c.value, label: c.label, description: c.description })) ?? []),
      ];
      return { items: allCommands.filter((c) => c.value.startsWith(prefix)), prefix: "/" };
    },
    applyCompletion(lines: string[], cursorLine: number, _cursorCol: number, item: { value: string; label: string }, prefix: string) {
      const line = lines[cursorLine] ?? "";
      const slashIdx = line.indexOf(prefix);
      const before = line.slice(0, slashIdx);
      const newLine = `${before + item.value} `;
      return { lines: [newLine], cursorLine, cursorCol: newLine.length };
    },
  };
}
