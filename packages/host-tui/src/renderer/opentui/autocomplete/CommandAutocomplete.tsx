// ============================================================================
// CommandAutocomplete — anchored autocomplete for slash commands
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { AutocompleteItem } from "../../../commands/types.js";

export interface CommandAutocompleteProps {
  items: AutocompleteItem[];
  query: string;
  selectedIndex: number;
  onSelect: (item: AutocompleteItem) => void;
  onCancel: () => void;
  maxVisible?: number;
}

export function CommandAutocomplete(props: CommandAutocompleteProps) {
  const theme = useTheme();

  const maxVisible = () => props.maxVisible ?? 8;
  const clampedIndex = () =>
    Math.max(0, Math.min(props.selectedIndex, props.items.length - 1));
  const visibleStart = () => {
    if (props.items.length <= maxVisible()) return 0;
    return Math.max(
      0,
      Math.min(
        clampedIndex() - Math.floor(maxVisible() / 2),
        props.items.length - maxVisible(),
      ),
    );
  };
  const visibleItems = () =>
    props.items.slice(visibleStart(), visibleStart() + maxVisible());

  return (
    <box
      flexDirection="column"
      border={["top"]}
      borderColor={theme.color("border.muted")}
      paddingLeft={1}
      paddingRight={1}
    >
      {visibleItems().length > 0 ? (
        visibleItems().map((item, i) => {
          const actualIndex = visibleStart() + i;
          const isSelected = actualIndex === clampedIndex();
          return (
            <box flexDirection="row" height={1}>
              <text
                fg={
                  isSelected
                    ? theme.color("text.accent")
                    : theme.color("text.primary")
                }
              >
                {isSelected ? "> " : "  "}{item.label}
              </text>
              {item.description && (
                <text fg={theme.color("text.dim")}>
                  {" — "}{item.description}
                </text>
              )}
            </box>
          );
        })
      ) : (
        <box height={1}>
          <text fg={theme.color("text.warning")}>No matching commands</text>
        </box>
      )}
    </box>
  );
}
