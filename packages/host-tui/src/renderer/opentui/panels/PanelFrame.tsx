import type { JSX } from "solid-js";
import type { PanelChrome } from "../../../panels/types.js";
import { useTheme } from "../theme-context.js";

export interface PanelFrameProps {
  chrome: PanelChrome;
  filterRow?: JSX.Element;
  placement?: "partial" | "full";
  children: JSX.Element;
}

export function PanelFrame(props: PanelFrameProps) {
  const theme = useTheme();
  
  return (
    <box flexDirection="column" width="100%" height="100%">
      <box flexDirection="column" flexGrow={1} overflow="hidden">
        {props.children}
      </box>
      
      {props.filterRow}
      
      {props.chrome.hints && props.chrome.hints.length > 0 && (
        <box paddingTop={0}>
          <text fg={theme.color("text.dim")}>{props.chrome.hints.join("  ")}</text>
        </box>
      )}
    </box>
  );
}
