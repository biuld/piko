// ============================================================================
// KeyValueList — single-column label-value list for settings-style UIs.
//
// Each row: label (left, truncated) + value (right-aligned).
// Selected row highlighted. Pure visual — no keyboard handling.
// ============================================================================

import { useTheme } from "../theme-context.js";
import { truncateToWidth, visibleWidth } from "../../../layout/measure.js";

export interface KeyValueItem {
  id: string;
  label: string;
  value: string;
}

export interface KeyValueListProps {
  items: KeyValueItem[];
  selectedIndex: number;
  maxVisible: number;
  width: number;
}

export function KeyValueList(props: KeyValueListProps) {
  const theme = useTheme();
  const { items, selectedIndex, maxVisible, width } = props;

  const labelMaxW = Math.min(28, Math.max(10, ...items.map((i) => visibleWidth(i.label))));
  const prefixW = 2; // "  " or "> "

  // Scroll window
  const start = Math.max(0, Math.min(selectedIndex - Math.floor(maxVisible / 2), items.length - maxVisible));
  const end = Math.min(start + maxVisible, items.length);
  const visible = items.slice(start, end);

  return (
    <box flexDirection="column">
      {visible.map((item, i) => {
        const idx = start + i;
        const isSelected = idx === selectedIndex;
        const prefix = isSelected ? "> " : "  ";

        const truncatedLabel = truncateToWidth(item.label, labelMaxW);
        const paddedLabel = truncatedLabel + " ".repeat(Math.max(0, labelMaxW - visibleWidth(truncatedLabel)));
        const valueMaxW = Math.max(4, width - prefixW - labelMaxW - 4);
        const truncatedValue = truncateToWidth(item.value, valueMaxW);

        return (
          <box
            flexDirection="row"
            height={1}
            backgroundColor={isSelected ? theme.color("surface.selected") : undefined}
          >
            <text fg={isSelected ? theme.color("text.accent") : theme.color("text.primary")}>
              {prefix + paddedLabel}
            </text>
            <text fg={isSelected ? theme.color("text.accent") : theme.color("text.dim")}>
              {"  " + truncatedValue}
            </text>
          </box>
        );
      })}
    </box>
  );
}
