import { Text } from "@earendil-works/pi-tui";
import type { Theme } from "../theme.js";
import type { ToolDef, ToolRenderContext } from "./types.js";

function renderCall(name: string, extract: (args: any) => string): ToolDef["renderCall"] {
  return (args: any, t: Theme, _ctx: ToolRenderContext) =>
    new Text(`${t.fg("toolTitle", t.bold(name))} ${t.fg("toolOutput", extract(args))}`, 0, 0);
}

export const writeDef: ToolDef = {
  name: "write",
  renderCall: renderCall("write", (a) => (a as { path?: string }).path ?? ""),
};

export const editDef: ToolDef = {
  name: "edit",
  renderCall: renderCall("edit", (a) => (a as { path?: string }).path ?? ""),
};

export const grepDef: ToolDef = {
  name: "grep",
  renderCall: renderCall("grep", (a) => {
    const p = (a as { pattern?: string }).pattern ?? "";
    return p.length > 160 ? `${p.slice(0, 157)}...` : p;
  }),
};

export const findDef: ToolDef = {
  name: "find",
  renderCall: renderCall("find", (a) => (a as { path?: string }).path ?? ""),
};

export const lsDef: ToolDef = {
  name: "ls",
  renderCall: renderCall("ls", (a) => (a as { path?: string }).path ?? ""),
};
