// ============================================================================
// CommandAutocomplete — anchored autocomplete for slash commands
// ============================================================================

import { createSignal, createMemo } from "solid-js";
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
  const {
    items,
    query,
    selectedIndex,
    onSelect,
    onCancel,
    maxVisible = 8,
  } = props;

  const visibleItems = () => items.slice(0, maxVisible);
  const clampedIndex = () => Math.max(0, Math.min(selectedIndex, items.length - 1));

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
          const isSelected = i === clampedIndex();
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
