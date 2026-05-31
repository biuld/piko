import { Container, Text } from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";
import type { ToolDef, ToolRenderContext } from "./types.js";

function truncate(s: string, max = 160): string {
  if (s.length <= max) return s;
  return `${s.slice(0, max - 3)}...`;
}

export const bashDef: ToolDef = {
  name: "bash",
  renderCall: (args: any, t: Theme, _ctx: ToolRenderContext) => {
    const cmd = (args as { command?: string }).command ?? "";
    return new Text(
      `${t.fg("bashMode", t.bold("$"))} ${t.fg("toolTitle", truncate(cmd, 120))}`,
      0,
      0,
    );
  },
  renderResult: (result, _opts, t: Theme, ctx: ToolRenderContext) => {
    const container = new Container();
    const output =
      typeof result.content === "string" ? result.content : JSON.stringify(result.content);

    if (!ctx.expanded) {
      // Collapsed: show first line summary
      const firstLine = output.split("\n")[0] ?? "";
      const truncated = firstLine.length > 200 ? `${firstLine.slice(0, 197)}...` : firstLine;
      const color = ctx.isError ? t.fg("error", truncated) : t.fg("toolOutput", truncated);
      container.addChild(new Text(color, 0, 0));
      return container;
    }

    // Expanded: show full output
    if (!output) {
      container.addChild(new Text(t.fg("dim", "(no output)"), 0, 0));
      return container;
    }

    const lines = output.split("\n");
    const maxLines = 500;
    const displayLines = lines.slice(0, maxLines);

    for (const line of displayLines) {
      const color = ctx.isError ? t.fg("error", line) : t.fg("toolOutput", line);
      container.addChild(new Text(color, 0, 0));
    }

    if (lines.length > maxLines) {
      container.addChild(
        new Text(t.fg("dim", `... (${lines.length - maxLines} more lines)`), 0, 0),
      );
    }

    return container;
  },
};
