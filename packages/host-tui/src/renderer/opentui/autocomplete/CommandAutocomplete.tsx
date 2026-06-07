// ============================================================================
// CommandAutocomplete — anchored autocomplete for slash commands
// ============================================================================

import { useTheme } from "../theme-context.js";
import type { AutocompleteItem } from "../../../commands/types.js";
import {
  clampListIndex,
  getSelectableListWindow,
} from "../../../surfaces/interactions/selectable-list.js";

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
    clampListIndex(props.selectedIndex, props.items.length);
  const visibleWindow = () =>
    getSelectableListWindow(props.items, props.selectedIndex, maxVisible());
  const visibleStart = () => visibleWindow().start;
  const visibleItems = () => visibleWindow().rows;

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
              {item.description ? (
                <text fg={theme.color("text.dim")}>
                  {" — "}{item.description}
                </text>
              ) : null}
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
