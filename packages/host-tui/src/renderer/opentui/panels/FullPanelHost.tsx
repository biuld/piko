import type { JSX } from "solid-js";

export interface FullPanelHostProps {
  children: JSX.Element;
}

export function FullPanelHost(props: FullPanelHostProps) {
  return (
    <box
      flexDirection="column"
      flexGrow={1}
      overflow="hidden"
      paddingLeft={1}
      paddingRight={1}
    >
      {props.children}
    </box>
  );
}
