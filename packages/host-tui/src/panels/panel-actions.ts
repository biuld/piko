import type { PanelRoute } from "./types.js";

export type PanelAction =
  | { type: "push_route"; route: PanelRoute<any> }
  | { type: "pop_route" }
  | { type: "replace_route"; route: PanelRoute<any> }
  | { type: "update_filter"; text: string }
  | { type: "update_selection"; index: number }
  | { type: "update_form"; values: Record<string, string> }
  | { type: "submit" }
  | { type: "cancel" };
