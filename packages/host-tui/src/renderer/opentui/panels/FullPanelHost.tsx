import type { JSX } from "solid-js";
import { useTheme } from "../theme-context.js";

export interface FullPanelHostProps {
  children: JSX.Element;
  title?: string;
  hints?: string[];
  filterRow?: JSX.Element;
}

export function FullPanelHost(props: FullPanelHostProps) {
  const theme = useTheme();
  const hasHeader = props.title || (props.hints && props.hints.length > 0);

  return (
    <box flexDirection="column" flexGrow={1} border={["bottom"]} borderColor={theme.color("border.accent")}>
      {/* Header: title + hints, top+bottom border */}
      {hasHeader ? (
        <box
          flexDirection="column"
          border={["top", "bottom"]}
          borderColor={theme.color("border.accent")}
          paddingLeft={1}
          paddingRight={1}
        >
          {props.title ? (
            <box height={1}>
              <text fg={theme.color("text.accent")}>{props.title}</text>
            </box>
          ) : null}
          {props.hints && props.hints.length > 0 ? (
            <box height={1}>
              <text fg={theme.color("text.dim")}>{props.hints.join("  ")}</text>
            </box>
          ) : null}
        </box>
      ) : null}

      {/* Content area */}
      <box
        flexDirection="column"
        flexGrow={1}
        overflow="hidden"
        paddingLeft={1}
        paddingRight={1}
        paddingBottom={1}
      >
        {props.filterRow ? (
          <>
            <box height={1} />
            {props.filterRow}
            <box height={1} />
          </>
        ) : (
          <box height={1} />
        )}
        {props.children}
      </box>
    </box>
  );
}
