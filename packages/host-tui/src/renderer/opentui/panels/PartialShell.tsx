// ============================================================================
// PartialShell — container for partial (inset) panels.
//
// Only responsible for: border, title overlay, fixed height, hints bar.
// ============================================================================

import type { JSX } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface PartialShellProps {
  children: JSX.Element;
  height: number;
  title?: string;
  hints?: string[];
}

export function PartialShell(props: PartialShellProps) {
  const theme = useTheme();

  return (
    <box
      flexDirection="column"
      flexShrink={0}
      height={props.height}
      border={["top", "bottom"]}
      borderColor={theme.color("border.accent")}
    >
      {props.title ? (
        <box position="absolute" top={-1} left={2} height={1}>
          <text fg={theme.color("text.accent")}> {props.title} </text>
        </box>
      ) : null}
      {props.children}
      {props.hints && props.hints.length > 0 ? (
        <box>
          <text fg={theme.color("text.dim")}>{`  ${props.hints.join("  ")}`}</text>
        </box>
      ) : null}
    </box>
  );
}
