// ============================================================================
// KeyValueList — single-column label-value list for settings-style UIs.
//
// Each row: label (left, padded) + value (right-aligned).
// Selected row highlighted. Pure visual — no keyboard handling.
// ============================================================================

import { truncateToWidth, visibleWidth } from "../../../layout/measure.js";
import { useTheme } from "../theme-context.js";

export interface KeyValueItem {
  id: string;
  label: string;
  value: string;
  /** Theme color key for the value text (default: "text.dim"). */
  valueColor?: string;
}

export interface KeyValueListProps {
  items: KeyValueItem[];
  selectedIndex: number;
  maxVisible: number;
  width: number;
}

export function KeyValueList(props: KeyValueListProps) {
  const theme = useTheme();
  const labelMaxW = () =>
    Math.min(28, Math.max(10, ...props.items.map((i) => visibleWidth(i.label))));
  const prefixW = 2;
  const start = () =>
    Math.max(
      0,
      Math.min(
        props.selectedIndex - Math.floor(props.maxVisible / 2),
        props.items.length - props.maxVisible,
      ),
    );
  const end = () => Math.min(start() + props.maxVisible, props.items.length);
  const visible = () => props.items.slice(start(), end());

  return (
    <box flexDirection="column">
      {visible().map((item, i) => {
        const idx = start() + i;
        const isSelected = idx === props.selectedIndex;
        const prefix = isSelected ? "> " : "  ";

        const truncatedLabel = truncateToWidth(item.label, labelMaxW());
        const paddedLabel =
          truncatedLabel + " ".repeat(Math.max(0, labelMaxW() - visibleWidth(truncatedLabel)));
        const valueMaxW = Math.max(4, props.width - prefixW - labelMaxW() - 4);
        const truncatedValue = truncateToWidth(item.value, valueMaxW);

        const labelFg = isSelected ? theme.color("text.accent") : theme.color("text.primary");
        const valueFg = item.valueColor
          ? theme.color(item.valueColor)
          : isSelected
            ? theme.color("text.accent")
            : theme.color("text.dim");

        return (
          <box
            flexDirection="row"
            height={1}
            backgroundColor={isSelected ? theme.color("surface.selected") : undefined}
          >
            <text fg={labelFg}>{prefix + paddedLabel}</text>
            <text fg={valueFg}>{`  ${truncatedValue}`}</text>
          </box>
        );
      })}
    </box>
  );
}
