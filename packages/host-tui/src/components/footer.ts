import type { Model } from "@earendil-works/pi-ai";
import { type Component, truncateToWidth, visibleWidth } from "@earendil-works/pi-tui";
import { getGitBranch } from "piko-host-runtime";
import { getTheme } from "../theme.js";

export interface FooterViewModel {
  model: Model<string>;
  sessionName?: string;
  messageCount: number;
  cwd: string;
  totalInputTokens?: number;
  totalOutputTokens?: number;
  totalCacheRead?: number;
  totalCacheWrite?: number;
  totalCost?: number;
  contextWindow?: number;
  contextPercent?: number;
  extensionStatuses?: string[];
  keyHints?: string;
}

function formatCwd(cwd: string, home: string | undefined): string {
  if (!home) return cwd;
  if (cwd.startsWith(home)) return `~${cwd.slice(home.length)}`;
  return cwd;
}

function formatTokens(count: number): string {
  if (count <= 0) return "";
  if (count < 1000) return count.toString();
  if (count < 10_000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  if (count < 10_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  return `${Math.round(count / 1_000_000)}M`;
}

export class FooterComponent implements Component {
  private view: FooterViewModel;
  private gitBranch: string | undefined;

  constructor(view: FooterViewModel) {
    this.view = view;
    this.gitBranch = getGitBranch(view.cwd);
  }

  update(view: FooterViewModel): void {
    const cwdChanged = view.cwd !== this.view.cwd;
    this.view = view;
    if (cwdChanged) {
      this.gitBranch = getGitBranch(view.cwd);
    }
  }

  invalidate(): void {}

  render(width: number): string[] {
    const t = getTheme();
    const {
      model,
      sessionName,
      messageCount,
      cwd,
      totalInputTokens,
      totalOutputTokens,
      totalCacheRead,
      totalCacheWrite,
      totalCost,
      contextWindow,
      contextPercent,
      extensionStatuses,
    } = this.view;
    const home = process.env.HOME || process.env.USERPROFILE;
    const lines: string[] = [];

    // Line 1: pwd • git branch • session name
    const pwd = formatCwd(cwd, home);
    const pathParts: string[] = [pwd];
    if (this.gitBranch) pathParts.push(`(${this.gitBranch})`);
    if (sessionName) pathParts.push(`• ${sessionName}`);
    lines.push(truncateToWidth(t.fg("dim", pathParts.join(" ")), width));

    // Line 2: token stats | model
    const statsParts: string[] = [];
    if (totalInputTokens) statsParts.push(t.fg("dim", `↑${formatTokens(totalInputTokens)}`));
    if (totalOutputTokens) statsParts.push(t.fg("dim", `↓${formatTokens(totalOutputTokens)}`));
    if (totalCacheRead) statsParts.push(t.fg("dim", `R${formatTokens(totalCacheRead)}`));
    if (totalCacheWrite) statsParts.push(t.fg("dim", `W${formatTokens(totalCacheWrite)}`));
    statsParts.push(t.fg("dim", `${messageCount} msgs`));
    if (totalCost !== undefined && totalCost > 0) {
      statsParts.push(t.fg("dim", `$${totalCost.toFixed(3)}`));
    }
    if (contextWindow && contextPercent !== undefined) {
      const pctStr = `${contextPercent.toFixed(1)}%/${formatTokens(contextWindow)}`;
      if (contextPercent > 90) {
        statsParts.push(t.fg("error", pctStr));
      } else if (contextPercent > 70) {
        statsParts.push(t.fg("warning", pctStr));
      } else {
        statsParts.push(t.fg("dim", pctStr));
      }
    }
    const statsLeft = statsParts.join(" ");

    const modelStr = `${model.provider}/${model.id}`;
    const modelStyled = t.fg("accent", modelStr);

    let statsLine: string;
    if (visibleWidth(statsLeft) + visibleWidth(modelStr) + 2 <= width) {
      const pad = " ".repeat(Math.max(1, width - visibleWidth(statsLeft) - visibleWidth(modelStr)));
      statsLine = `${statsLeft}${pad}${modelStyled}`;
    } else {
      statsLine = truncateToWidth(`${statsLeft}  ${modelStyled}`, width);
    }
    lines.push(truncateToWidth(statsLine, width));

    // Line 3: extension statuses
    if (extensionStatuses && extensionStatuses.length > 0) {
      const statusLine = extensionStatuses.join(" ");
      lines.push(truncateToWidth(statusLine, width));
    }

    // Line 4: dynamic key hints
    const { keyHints } = this.view;
    if (keyHints) {
      lines.push(truncateToWidth(keyHints, width));
    }

    return lines;
  }
}
