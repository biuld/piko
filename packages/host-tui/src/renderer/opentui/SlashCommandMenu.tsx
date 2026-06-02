// ============================================================================
// SlashCommandMenu — lightweight command autocomplete for the editor
// ============================================================================

import type { SlashCommand } from "./keybinding-registry.js";
import { useTheme } from "./theme-context.js";

export interface SlashCommandMenuProps {
  commands: SlashCommand[];
  query: string;
  maxVisible?: number;
}

function matchesCommand(command: SlashCommand, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q || q === "/") return true;
  return (
    command.name.toLowerCase().startsWith(q) ||
    command.aliases?.some((alias) => alias.toLowerCase().startsWith(q)) === true
  );
}

function commandRank(command: SlashCommand, query: string): number {
  const q = query.trim().toLowerCase();
  if (!q || q === "/") return 0;
  if (command.name.toLowerCase() === q) return 0;
  if (command.name.toLowerCase().startsWith(q)) return 1;
  if (command.aliases?.some((alias) => alias.toLowerCase() === q)) return 2;
  return 3;
}

export function SlashCommandMenu(props: SlashCommandMenuProps) {
  const theme = useTheme();

  const visibleCommands = () =>
    props.commands
      .filter((command) => matchesCommand(command, props.query))
      .sort((a, b) => commandRank(a, props.query) - commandRank(b, props.query) || a.name.localeCompare(b.name))
      .slice(0, props.maxVisible ?? 6);

  return (
    <box
      flexDirection="column"
      border={["top"]}
      borderColor={theme.color("border.muted")}
      paddingLeft={1}
      paddingRight={1}
    >
      {visibleCommands().length > 0 ? (
        visibleCommands().map((command) => (
          <box flexDirection="row" height={1}>
            <text fg={theme.color("text.accent")}>{command.name}</text>
            <text fg={theme.color("text.dim")}>
              {command.aliases?.length ? ` ${command.aliases.join(",")} ` : " "}
              {command.description}
            </text>
          </box>
        ))
      ) : (
        <box height={1}>
          <text fg={theme.color("text.warning")}>No matching commands</text>
        </box>
      )}
    </box>
  );
}
