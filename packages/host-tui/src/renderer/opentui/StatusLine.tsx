// ============================================================================
// StatusLine — shows streaming progress / tool status / queue info
// ============================================================================

import { useTheme } from "./theme-context.js";
import { Show } from "solid-js";

export interface StatusLineProps {
  entries: string[];
  visible: boolean;
}

export function StatusLine(props: StatusLineProps) {
  const theme = useTheme();

  return (
    <Show when={props.visible}>
      <box flexShrink={0} height={1} paddingLeft={1} paddingRight={1}>
        <text fg={theme.color("text.muted")}>{props.entries.join(" │ ")}</text>
      </box>
    </Show>
  );
}
