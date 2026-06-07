export interface PanelSession {
  id: string;
  stack: PanelRoute<any>[];
  state: PanelState;
}

export interface PanelState {
  selectedIndex?: number;
  filterText?: string;
  formValues?: Record<string, string>;
}

export interface PanelRoute<TPayload = unknown> {
  id: string;
  chrome: PanelChrome;
  interaction: PanelInteraction;
  capabilities: PanelCapability[];
  body: PanelBody<TPayload>;
}

export interface PanelChrome {
  title: string;
  hints?: string[];
}

export type PanelCapability =
  | { kind: "filter"; placeholder?: string }
  | { kind: "list"; selectable: boolean }
  | { kind: "form" }
  | { kind: "detail" };

export type PanelInteraction = "list" | "menu" | "form" | "confirm" | "passive";

export type PanelBodyType =
  | "model-picker"
  | "thinking-picker"
  | "session-resume"
  | "settings"
  | "login"
  | "notifications"
  | "hotkeys"
  | "help"
  | "changelog"
  | "session-info"
  | "session-tree"
  | "session-fork"
  | "session-import"
  | "session-rename";

export interface PanelBody<TPayload = unknown> {
  type: PanelBodyType;
  payload: TPayload;
}
