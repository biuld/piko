import { type Component, truncateToWidth } from "@earendil-works/pi-tui";
import type { EngineModel } from "piko-engine-protocol";

export interface FooterViewModel {
  model: EngineModel;
  sessionName?: string;
  messageCount: number;
  cwd: string;
}

function formatCwd(cwd: string, home: string | undefined): string {
  if (!home) return cwd;
  if (cwd.startsWith(home)) return `~${cwd.slice(home.length)}`;
  return cwd;
}

export class FooterComponent implements Component {
  private view: FooterViewModel;

  constructor(view: FooterViewModel) {
    this.view = view;
  }

  update(view: FooterViewModel): void {
    this.view = view;
  }

  invalidate(): void {}

  render(width: number): string[] {
    const { model, sessionName, messageCount, cwd } = this.view;
    const home = process.env.HOME || process.env.USERPROFILE;
    const pwd = formatCwd(cwd, home);

    // Line 1: cwd • session name
    const pathParts: string[] = [pwd];
    if (sessionName) pathParts.push(`• ${sessionName}`);
    const pathLine = truncateToWidth(pathParts.join(" "), width);

    // Line 2: model | messages
    const modelStr = `${model.provider}/${model.id}`;
    const rightStr = `${messageCount} msgs`;
    const padLen = Math.max(1, width - modelStr.length - rightStr.length);
    const pad = " ".repeat(padLen);
    const statsLine = `${modelStr}${pad}${rightStr}`;

    return [pathLine, truncateToWidth(statsLine, width)];
  }
}
