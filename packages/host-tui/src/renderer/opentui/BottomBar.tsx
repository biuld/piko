// ============================================================================
// BottomBar — session info, model, token usage, key hints
// ============================================================================

import {
  selectBottomBarFields,
  selectFormattedCost,
  selectFormattedInputTokens,
  selectFormattedOutputTokens,
} from "../../state/selectors.js";
import { useTheme } from "./theme-context.js";
import type { TuiStore } from "./store.js";

export interface BottomBarProps {
  store: TuiStore;
}

function formatCwd(cwd: string): string {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (home && cwd.startsWith(home)) {
    return `~${cwd.slice(home.length)}`;
  }
  return cwd;
}

export function BottomBar(props: BottomBarProps) {
  const theme = useTheme();
  const state = props.store.state;
  const session = () => state().session;
  const model = () => state().model;
  const usage = () => state().usage;
  const layout = () => state().layout;

  const fields = () => selectBottomBarFields(state());
  const inputTokens = () => selectFormattedInputTokens(state());
  const outputTokens = () => selectFormattedOutputTokens(state());
  const cost = () => selectFormattedCost(state());

  const visible = (field: string) => fields().includes(field as any);

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1}>
      {/* Line 1: cwd • git branch • session name */}
      {visible("cwd") || visible("branch") || visible("session") ? (
        <box flexDirection="row" height={1}>
          {visible("cwd") && (
            <text fg={theme.color("text.dim")}>{formatCwd(session().cwd)}</text>
          )}
          {visible("branch") && session().gitBranch && (
            <text fg={theme.color("text.dim")}> ({session().gitBranch})</text>
          )}
          {visible("session") && session().sessionName && (
            <text fg={theme.color("text.dim")}> • {session().sessionName}</text>
          )}
        </box>
      ) : null}

      {/* Line 2: token usage | model */}
      {layout().bottomBar.density !== "minimal" ? (
        <box flexDirection="row" height={1}>
          {visible("tokens") && (
            <>
              {inputTokens() && <text fg={theme.color("text.dim")}>↑{inputTokens()} </text>}
              {outputTokens() && <text fg={theme.color("text.dim")}>↓{outputTokens()} </text>}
            </>
          )}
          {visible("cost") && cost() && (
            <text fg={theme.color("text.dim")}>{cost()} </text>
          )}
          <text fg={theme.color("text.dim")}>{session().messageCount} msgs</text>
          <text fg={theme.color("text.dim")}> </text>
          {visible("model") && (
            <text fg={theme.color("text.accent")}>{model().current.provider}/{model().current.id}</text>
          )}
        </box>
      ) : visible("model") ? (
        <box flexDirection="row" height={1}>
          <text fg={theme.color("text.accent")}>{model().current.provider}/{model().current.id}</text>
        </box>
      ) : null}
    </box>
  );
}
