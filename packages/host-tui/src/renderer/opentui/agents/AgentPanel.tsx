import { Index } from "solid-js";
import { getAgentPanelColumns } from "../../../agents/agent-panel-layout.js";
import { buildAgentPanelRows } from "../../../agents/agent-panel-model.js";
import type {
  AgentPanelMode,
  AgentPanelRowTone,
  AgentPanelSelectEvent,
  AgentPanelViewModel,
} from "../../../agents/types.js";
import { truncateToWidth } from "../../../layout/measure.js";
import { Spinner } from "../status/Spinner.js";
import { useTheme } from "../theme-context.js";

export interface AgentPanelProps {
  agent: AgentPanelViewModel;
  mode: AgentPanelMode;
  width: number;
  selected?: boolean;
  onSelect?: (event: AgentPanelSelectEvent) => void;
  spinnerFrame?: number;
}

/** Embeddable agent activity rows. The parent owns borders, placement and expansion state. */
export function AgentPanel(props: AgentPanelProps) {
  const theme = useTheme();
  const rows = () => buildAgentPanelRows(props.agent, props.mode);
  const columns = () => getAgentPanelColumns(props.width);

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: OpenTUI boxes are terminal hit targets, not DOM elements.
    <box
      flexDirection="column"
      flexShrink={0}
      height={rows().length}
      overflow="hidden"
      onMouseDown={() => props.onSelect?.({ type: "agent_selected", agentId: props.agent.id })}
    >
      <Index each={rows()}>
        {(row) => {
          const tone = () =>
            theme.color(
              props.selected && row().kind === "agent" ? "text.accent" : rowToneToken(row().tone),
            );
          return (
            <box height={1} flexDirection="row" width={props.width} overflow="hidden">
              <box width={columns().marker} overflow="hidden">
                <text>{"  ".repeat(row().indent)}</text>
                {row().spinner ? (
                  <Spinner frame={props.spinnerFrame} trailingSpace={false} fg={tone()} />
                ) : (
                  <text fg={tone()}>{row().icon}</text>
                )}
              </box>
              <box width={columns().name} overflow="hidden">
                <text fg={tone()}>{truncateToWidth(row().name ?? "", columns().name)}</text>
              </box>
              <box width={columns().progress} overflow="hidden">
                <text fg={tone()}>{truncateToWidth(row().progress ?? "", columns().progress)}</text>
              </box>
              <box width={columns().detail} overflow="hidden">
                <text fg={tone()}>{truncateToWidth(row().detail ?? "", columns().detail)}</text>
              </box>
              {columns().queue > 0 && (
                <box width={columns().queue} overflow="hidden">
                  <text fg={theme.color("text.dim")}>
                    {truncateToWidth(row().queue ?? "", columns().queue)}
                  </text>
                </box>
              )}
            </box>
          );
        }}
      </Index>
    </box>
  );
}

function rowToneToken(tone: AgentPanelRowTone): string {
  if (tone === "accent") return "text.accent";
  if (tone === "success") return "text.success";
  if (tone === "error") return "text.error";
  if (tone === "muted") return "text.dim";
  return "text.primary";
}
