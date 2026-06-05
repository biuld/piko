// ============================================================================
// InputRouter — single business key routing pipeline
// ============================================================================

import type { FocusManager } from "./focus-manager.js";
import type { KeyEvent } from "./types.js";

export interface InputRouterOptions {
  focus: FocusManager;
  getState: () => { surfaces: Array<any> };
  appFallback: (event: KeyEvent) => boolean;
}

export class InputRouter {
  private editorChildHandler: ((event: KeyEvent) => boolean) | null = null;

  constructor(private options: InputRouterOptions) {}

  setEditorChildHandler(handler: ((event: KeyEvent) => boolean) | null): void {
    this.editorChildHandler = handler;
  }

  dispatch(event: KeyEvent): boolean {
    if (
      this.editorChildHandler &&
      this.options.focus.isFocused("editor") &&
      this.editorChildHandler(event)
    ) {
      return true;
    }

    if (this.options.focus.handleKey(event)) return true;

    if (
      this.options
        .getState()
        .surfaces.some((s) => ("blocking" in s ? s.blocking : s.inputPolicy !== "passive"))
    ) {
      return false;
    }

    return this.options.appFallback(event);
  }
}
