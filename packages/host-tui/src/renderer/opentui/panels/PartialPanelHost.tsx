import type { JSX } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface PartialPanelHostProps {
  children: JSX.Element;
  height?: number;
  title?: string;
}

export function PartialPanelHost(props: PartialPanelHostProps) {
  const theme = useTheme();
  const h = props.height ?? 14;

  return (
    <box
      flexDirection="column"
      flexShrink={0}
      height={h}
      border={["top", "bottom"]}
      borderColor={theme.color("border.accent")}
      paddingLeft={1}
      paddingRight={1}
    >
      {props.title && (
        <box position="absolute" top={-1} left={2} height={1}>
          <text fg={theme.color("text.accent")}> {props.title} </text>
        </box>
      )}
      {props.children}
    </box>
  );
}
