// ============================================================================
// Focus types — focus ownership model
// ============================================================================

export type FocusRegion = "editor" | "chat" | "surface" | "confirm";

export type FocusResult =
  | { handled: true }
  | { handled: false }
  | { push: FocusNode }
  | { pop: true }
  | { popTo: string };

export interface FocusOwner {
  id: string;
  region: FocusRegion;
  priority: number;
  handleText?: (text: string) => boolean;
  handleKey?: (event: KeyEvent) => FocusResult;
  interceptors?: FocusInterceptor[];
  focus?: () => void;
  blur?: () => void;
}

export interface FocusInterceptor {
  id: string;
  priority: number;
  match: (event: KeyEvent, state: any) => boolean;
  handle: (event: KeyEvent, state: any) => FocusResult;
}

export interface FocusNode {
  id: string;
  region: FocusRegion;
  parentId?: string;
  blocking: boolean;
  restoreTo?: string;
  handleKey?: (event: KeyEvent) => FocusResult;
}

export interface TuiFocusState {
  activeOwnerId: string;
  stack: string[];
  region: FocusRegion;
  path: string[];
}

export interface KeyEvent {
  name: string;
  ctrl: boolean;
  shift: boolean;
  alt?: boolean;
  meta?: boolean;
  char?: string;
}
