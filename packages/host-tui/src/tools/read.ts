import { Text } from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";
import type { ToolDef, ToolRenderContext } from "./types.js";

export const readDef: ToolDef = {
  name: "read",
  renderCall: (args: any, t: Theme, _ctx: ToolRenderContext) => {
    const path = (args as { path?: string }).path ?? "";
    return new Text(`${t.fg("toolTitle", t.bold("read"))} ${t.fg("toolOutput", path)}`, 0, 0);
  },
};
