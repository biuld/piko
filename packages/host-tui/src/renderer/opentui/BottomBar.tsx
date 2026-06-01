// ============================================================================
// BottomBar — session info, model, token usage, key hints
// ============================================================================

import { selectFormattedInputTokens, selectFormattedOutputTokens, selectFormattedCost, selectBottomBarFields } from "../../state/selectors.js";
import type { TuiStore } from "./store.js";

export interface BottomBarProps {
  store: TuiStore;
}

/**
 * Format cwd for display — replace HOME with ~
 */
function formatCwd(cwd: string): string {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (home && cwd.startsWith(home)) {
    return `~${cwd.slice(home.length)}`;
  }
  return cwd;
}

export function BottomBar(props: BottomBarProps) {
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
            <text fg="#666666">{formatCwd(session().cwd)}</text>
          )}
          {visible("branch") && session().gitBranch && (
            <text fg="#666666"> ({session().gitBranch})</text>
          )}
          {visible("session") && session().sessionName && (
            <text fg="#666666"> • {session().sessionName}</text>
          )}
        </box>
      ) : null}

      {/* Line 2: token usage | model */}
      {layout().bottomBar.density !== "minimal" ? (
        <box flexDirection="row" height={1}>
          {visible("tokens") && (
            <>
              {inputTokens() && <text fg="#666666">↑{inputTokens()} </text>}
              {outputTokens() && <text fg="#666666">↓{outputTokens()} </text>}
            </>
          )}
          {visible("cost") && cost() && (
            <text fg="#666666">{cost()} </text>
          )}
          <text fg="#666666">{session().messageCount} msgs</text>
          <text fg="#666666"> </text>
          {visible("model") && (
            <text fg="#8abeb7">{model().current.provider}/{model().current.id}</text>
          )}
        </box>
      ) : visible("model") ? (
        <box flexDirection="row" height={1}>
          <text fg="#8abeb7">{model().current.provider}/{model().current.id}</text>
        </box>
      ) : null}

      {/* Line 3-4: extension statuses (reserved for future) */}
    </box>
  );
}
