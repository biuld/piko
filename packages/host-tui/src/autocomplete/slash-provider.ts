// ============================================================================
// SlashCommandAutocompleteProvider — provides completions for slash commands
// ============================================================================

import type { CommandRegistry } from "../commands/command-registry.js";
import type { AutocompleteItem, AutocompleteProvider, AutocompleteSuggestions } from "./types.js";

export class SlashCommandAutocompleteProvider implements AutocompleteProvider {
  constructor(private registry: CommandRegistry) {}

  async getSuggestions(
    input: string,
    _cursor: number,
    _options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null> {
    const trimmed = input.trim();
    if (!trimmed.startsWith("/")) return null;

    // If there's a space, we're completing arguments
    const spaceIndex = trimmed.indexOf(" ");
    if (spaceIndex > 0) {
      const cmdText = trimmed.slice(0, spaceIndex);
      const cmd = this.registry.findBySlash(cmdText);
      if (cmd?.slash?.argumentHint) {
        return {
          prefix: cmdText,
          providerId: "slash",
          items: [
            {
              value: cmd.slash.name,
              label: cmd.slash.name,
              providerId: "slash",
              description: `${cmd.slash.argumentHint} — ${cmd.slash.description}`,
            },
          ],
        };
      }
      return null;
    }

    // Completing command name
    const prefix = trimmed.toLowerCase();
    const commands = this.registry
      .listSlashCommands()
      .filter((cmd) => {
        if (cmd.name.toLowerCase().startsWith(prefix)) return true;
        return cmd.aliases?.some((a) => a.toLowerCase().startsWith(prefix)) ?? false;
      })
      .sort((a, b) => {
        if (a.name.toLowerCase() === prefix) return -1;
        if (b.name.toLowerCase() === prefix) return 1;
        return a.name.localeCompare(b.name);
      })
      .map(
        (cmd): AutocompleteItem => ({
          value: cmd.name,
          label: cmd.name,
          providerId: "slash",
          description: `${cmd.description}${
            cmd.aliases?.length ? ` (${cmd.aliases.join(", ")})` : ""
          }`,
        }),
      );

    if (commands.length === 0) return null;
    return { prefix, providerId: "slash", items: commands };
  }

  applyCompletion(
    input: string,
    _cursor: number,
    item: AutocompleteItem,
    _prefix: string,
  ): { input: string; cursor: number } {
    // Only handle slash completions — bail out for file or other providers
    const trimmed = input.trimStart();
    if (!trimmed.startsWith("/")) return { input, cursor: _cursor };
    const leadingSpace = input.slice(0, input.length - trimmed.length);
    const spaceIdx = trimmed.indexOf(" ");
    if (spaceIdx > 0) {
      // Replace the command part with the selected command (keep args)
      const newText = leadingSpace + item.value + trimmed.slice(spaceIdx);
      return { input: newText, cursor: newText.length };
    }
    // Replace entire input with the selected command + space for next arg
    const newText = `${leadingSpace}${item.value} `;
    return { input: newText, cursor: newText.length };
  }
}
