// ============================================================================
// Slash command autocomplete provider
// ============================================================================

import type { CommandRegistry } from "./command-registry.js";
import type { AutocompleteItem } from "./types.js";

export class SlashCommandProvider {
  constructor(private registry: CommandRegistry) {}

  /**
   * Get autocomplete suggestions for a slash command prefix.
   */
  getSuggestions(input: string): AutocompleteItem[] {
    const trimmed = input.trim();
    if (!trimmed.startsWith("/")) return [];

    // If there's a space, we're completing arguments
    const spaceIndex = trimmed.indexOf(" ");
    if (spaceIndex > 0) {
      const cmdText = trimmed.slice(0, spaceIndex);
      const argPrefix = trimmed.slice(spaceIndex + 1);
      const cmd = this.registry.findBySlash(cmdText);

      if (cmd?.slash?.argumentHint) {
        return [
          {
            value: argPrefix,
            label: `${cmd.slash.argumentHint}`,
            description: cmd.slash.description,
          },
        ];
      }
      return [];
    }

    // Completing command name
    const prefix = trimmed.toLowerCase();
    return this.registry
      .listSlashCommands()
      .filter((cmd) => {
        if (cmd.name.toLowerCase().startsWith(prefix)) return true;
        return cmd.aliases?.some((a) => a.toLowerCase().startsWith(prefix)) ?? false;
      })
      .sort((a, b) => {
        // Exact matches first
        if (a.name.toLowerCase() === prefix) return -1;
        if (b.name.toLowerCase() === prefix) return 1;
        return a.name.localeCompare(b.name);
      })
      .map((cmd) => ({
        value: cmd.name,
        label: cmd.name,
        description: `${cmd.description}${cmd.aliases?.length ? ` (${cmd.aliases.join(", ")})` : ""}`,
      }));
  }
}
