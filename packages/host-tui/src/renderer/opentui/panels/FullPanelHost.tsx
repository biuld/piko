import type { JSX } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface FullPanelHostProps {
  children: JSX.Element;
  title?: string;
}

export function FullPanelHost(props: FullPanelHostProps) {
  const theme = useTheme();

  return (
    <box
      flexDirection="column"
      flexGrow={1}
      border={["bottom"]}
      borderColor={theme.color("border.accent")}
    >
      {props.title ? (
        <box paddingLeft={1} height={1}>
          <text fg={theme.color("text.accent")}>{props.title}</text>
        </box>
      ) : null}
      <box
        flexDirection="column"
        flexGrow={1}
        overflow="hidden"
        paddingLeft={1}
        paddingRight={1}
      >
        {props.children}
      </box>
    </box>
  );
}
