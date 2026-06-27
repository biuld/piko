import { TextAttributes } from "@opentui/core";
import { createSignal, For, onMount, Show } from "solid-js";
import type { TuiContextFile, TuiHostFacade } from "../../../app/tui-host.js";
import { useTheme } from "../theme-context.js";

export interface WelcomeBannerProps {
  host: TuiHostFacade;
  width: number;
}

export function WelcomeBanner(props: WelcomeBannerProps) {
  const theme = useTheme();
  const [contextFiles, setContextFiles] = createSignal<TuiContextFile[]>([]);

  onMount(() => {
    void (async () => {
      try {
        const files = await props.host.getContextFiles();
        setContextFiles(files);
      } catch {
        // ignore
      }
    })();
  });

  const contentWidth = () => Math.min(props.width - 6, 75);

  return (
    <box
      flexDirection="column"
      paddingTop={2}
      paddingBottom={2}
      paddingLeft={3}
      paddingRight={3}
      width={props.width}
    >
      {/* Overall Border Box */}
      <box
        flexDirection="row"
        border={["top", "bottom", "left", "right"]}
        borderColor={theme.color("border.muted")}
        paddingTop={1}
        paddingBottom={1}
        paddingLeft={2}
        paddingRight={2}
        width={contentWidth()}
      >
        {/* Left Column: Logo & Version */}
        <box flexDirection="column" width={22} flexShrink={0} paddingRight={2}>
          <box flexDirection="column" paddingBottom={1}>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {"  ____  _ _"}
            </text>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {" |  _ \\(_) | _ ___"}
            </text>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {" | |_) | | |/ / _ \\"}
            </text>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {" |  __/| |   < (_) |"}
            </text>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {" |_|   |_|_|\\_\\___/"}
            </text>
          </box>
          <box height={1}>
            <text fg={theme.color("text.dim")}>{`v${props.host.version}`}</text>
          </box>
        </box>

        {/* Right Column: Statistics & Context Files */}
        <box
          flexDirection="column"
          flexGrow={1}
          paddingLeft={2}
          border={["left"]}
          borderColor={theme.color("border.muted")}
        >
          {/* Section 1: System Status */}
          <box height={1}>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {"System Environment:"}
            </text>
          </box>
          <box flexDirection="row" height={1}>
            <text fg={theme.color("text.primary")}>{"• Model: "}</text>
            <text fg={theme.color("text.dim")}>
              {props.host.getConfig().model.name || props.host.getConfig().model.id}
            </text>
          </box>
          <box flexDirection="row" height={1}>
            <text fg={theme.color("text.primary")}>{"• Active Tools: "}</text>
            <text fg={theme.color("text.dim")}>
              {`${(() => {
                const active = props.host.getActiveToolNames();
                return active !== undefined ? active.length : props.host.getTotalToolCount();
              })()} enabled`}
            </text>
          </box>

          <box height={1} />
          <box height={1} border={["bottom"]} borderColor={theme.color("border.muted")} />
          <box height={1} />

          {/* Section 2: Context Files */}
          <box height={1}>
            <text fg={theme.color("text.accent")} attributes={TextAttributes.BOLD}>
              {"Detected Context Files:"}
            </text>
          </box>
          <box height={1} />

          <Show
            when={contextFiles().length > 0}
            fallback={
              <box height={1}>
                <text fg={theme.color("text.dim")}>
                  {"No context files (e.g. AGENTS.md, CLAUDE.md) detected."}
                </text>
              </box>
            }
          >
            <For each={contextFiles()}>
              {(file) => {
                const name = file.path.split("/").pop() || file.path;
                let relPath = file.path;
                if (file.path.startsWith(props.host.cwd)) {
                  relPath = `.${file.path.slice(props.host.cwd.length)}`;
                }
                const maxPathLen = contentWidth() - name.length - 34;
                let pathStr = `(${relPath})`;
                if (pathStr.length > maxPathLen && maxPathLen > 10) {
                  pathStr = `(...${pathStr.slice(-maxPathLen + 4)}`;
                }
                return (
                  <box flexDirection="row" height={1}>
                    <text fg={theme.color("text.primary")} attributes={TextAttributes.BOLD}>
                      {`- ${name} `}
                    </text>
                    <text fg={theme.color("text.dim")}>{pathStr}</text>
                  </box>
                );
              }}
            </For>
          </Show>
        </box>
      </box>
    </box>
  );
}
