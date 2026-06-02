// ============================================================================
// BottomBar — session info, model, token usage, key hints
// ============================================================================

import {
  selectBottomBarFields,
  selectContextInfo,
  selectFormattedCost,
  selectFormattedInputTokens,
  selectFormattedOutputTokens,
} from "../../state/selectors.js";
import { middleTruncate, packBottomBar } from "../../layout/bottom-bar-packer.js";
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
  const width = () => Math.max(20, layout().viewport.width - 2);

  const fields = () => selectBottomBarFields(state());
  const inputTokens = () => selectFormattedInputTokens(state());
  const outputTokens = () => selectFormattedOutputTokens(state());
  const cost = () => selectFormattedCost(state());
  const contextInfo = () => selectContextInfo(state());

  const visible = (field: string) => fields().includes(field as any);
  const cacheReadTokens = () => formatTokens(usage().cacheReadTokens);
  const cacheWriteTokens = () => formatTokens(usage().cacheWriteTokens);
  const contextParts = () => {
    const info = contextInfo();
    if (!info) return { percent: undefined, window: undefined };
    const [percent, window] = info.split("/");
    return { percent, window };
  };
  const lines = () => {
    const context = contextParts();
    return packBottomBar(
      {
        cwd: visible("cwd") ? middleTruncate(formatCwd(session().cwd), Math.max(12, Math.floor(width() * 0.45))) : "",
        gitBranch: visible("branch") ? session().gitBranch : undefined,
        sessionName: visible("session") ? session().sessionName : undefined,
        modelProvider: model().current.provider,
        modelId: model().current.id,
        thinkingLevel: model().thinkingLevel,
        inputTokens: visible("tokens") ? inputTokens() : "",
        outputTokens: visible("tokens") ? outputTokens() : "",
        cacheReadTokens: visible("tokens") ? cacheReadTokens() : "",
        cacheWriteTokens: visible("tokens") ? cacheWriteTokens() : "",
        cost: visible("cost") ? cost() : "",
        contextPercent: context.percent,
        contextWindow: context.window,
        messageCount: session().messageCount,
      },
      width(),
    );
  };
  const minimalLine = () => {
    const cwd = middleTruncate(formatCwd(session().cwd), Math.max(8, Math.floor(width() * 0.45)));
    const modelText = `${model().current.provider}/${model().current.id}`;
    return packSingleLine(cwd, modelText, width());
  };

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1}>
      <box height={1}>
        <text fg={theme.color("text.dim")}>
          {layout().bottomBar.density === "minimal" ? minimalLine() : lines().line1}
        </text>
      </box>
      {layout().bottomBar.density !== "minimal" && (
        <box height={1}>
          <text fg={theme.color("text.dim")}>{lines().line2}</text>
        </box>
      )}
    </box>
  );
}

function packSingleLine(left: string, right: string, width: number): string {
  if (width <= 0) return "";
  if (!right) return left.slice(0, width);
  const availableLeft = Math.max(0, width - right.length - 2);
  if (availableLeft <= 0) return right.slice(0, width);
  const truncatedLeft = left.length > availableLeft ? `${left.slice(0, Math.max(0, availableLeft - 3))}...` : left;
  const padding = Math.max(2, width - truncatedLeft.length - right.length);
  return `${truncatedLeft}${" ".repeat(padding)}${right}`.slice(0, width);
}

function formatTokens(count: number): string {
  if (count <= 0) return "";
  if (count < 1000) return count.toString();
  if (count < 10_000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  if (count < 10_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  return `${Math.round(count / 1_000_000)}M`;
}
