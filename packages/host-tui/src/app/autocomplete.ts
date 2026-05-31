import type { AutocompleteProvider, AutocompleteSuggestions } from "@earendil-works/pi-tui";
import type { ExtensionHost } from "../extensions/index.js";
import { COMMANDS } from "../commands/index.js";

export function createAutocomplete(extensionHost?: ExtensionHost): AutocompleteProvider {
  return {
    async getSuggestions(lines: string[], cursorLine: number, cursorCol: number): Promise<AutocompleteSuggestions | null> {
      const line = lines[cursorLine] ?? "";
      const prefix = line.slice(0, cursorCol);
      if (!prefix.startsWith("/")) return null;
      const all = [...COMMANDS, ...(extensionHost?.commands.map(c => ({ value: c.value, label: c.label, description: c.description })) ?? [])];
      return { items: all.filter(c => c.value.startsWith(prefix)), prefix: "/" };
    },
    applyCompletion(lines: string[], cursorLine: number, _cc: number, item: { value: string; label: string }, prefix: string) {
      const line = lines[cursorLine] ?? "";
      const idx = line.indexOf(prefix);
      const nl = `${line.slice(0, idx) + item.value} `;
      return { lines: [nl], cursorLine, cursorCol: nl.length };
    },
  };
}
