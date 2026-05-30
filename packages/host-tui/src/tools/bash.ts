import { Text } from "@earendil-works/pi-tui";
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
      `${t.fg("toolTitle", t.bold("bash"))} ${t.fg("toolOutput", truncate(cmd, 160))}`,
      0,
      0,
    );
  },
};
