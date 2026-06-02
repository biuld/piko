// ============================================================================
// FocusManager — focus tree, nested menus, interceptors, restore behavior
// ============================================================================

import type { FocusOwner, FocusRegion, FocusResult, KeyEvent, TuiFocusState } from "./types.js";

export class FocusManager {
  private state: TuiFocusState = {
    activeOwnerId: "editor",
    stack: ["editor"],
    region: "editor",
    path: ["editor"],
  };

  private owners: Map<string, FocusOwner> = new Map();

  /** Global key handler for emergency keys (interrupt/exit) */
  private globalHandler?: (event: KeyEvent) => boolean;

  /** External state accessor for interceptor matching */
  private stateAccessor?: () => any;

  constructor() {
    this.registerOwner({
      id: "editor",
      region: "editor",
      priority: 0,
    });
  }

  registerOwner(owner: FocusOwner): void {
    this.owners.set(owner.id, owner);
  }

  unregisterOwner(id: string): void {
    if (id === "editor") return;
    this.owners.delete(id);
    if (this.state.activeOwnerId === id) {
      this.restoreFocus();
    }
  }

  setGlobalHandler(handler: (event: KeyEvent) => boolean): void {
    this.globalHandler = handler;
  }

  setStateAccessor(fn: () => any): void {
    this.stateAccessor = fn;
  }

  getState(): TuiFocusState {
    return { ...this.state, path: [...this.state.path], stack: [...this.state.stack] };
  }

  pushFocus(id: string, region: FocusRegion, _restoreTo?: string): void {
    const prevId = this.state.activeOwnerId;
    const prevOwner = this.owners.get(prevId);
    prevOwner?.blur?.();

    this.state.stack.push(id);
    this.state.path.push(id);
    this.state.activeOwnerId = id;
    this.state.region = region;

    const owner = this.owners.get(id);
    owner?.focus?.();
  }

  popFocus(): void {
    if (this.state.stack.length <= 1) return;

    const currId = this.state.stack.pop()!;
    this.state.path.pop();

    const currOwner = this.owners.get(currId);
    currOwner?.blur?.();

    this.state.activeOwnerId = this.state.stack[this.state.stack.length - 1];
    const restoreOwner = this.owners.get(this.state.activeOwnerId);
    this.state.region = restoreOwner?.region ?? "editor";
    restoreOwner?.focus?.();
  }

  popToFocus(id: string): void {
    const idx = this.state.stack.indexOf(id);
    if (idx < 0) return;

    while (this.state.stack.length > idx + 1) {
      const popped = this.state.stack.pop()!;
      const poppedOwner = this.owners.get(popped);
      poppedOwner?.blur?.();
      this.state.path.pop();
    }

    this.state.activeOwnerId = id;
    const owner = this.owners.get(id);
    this.state.region = owner?.region ?? "editor";
    owner?.focus?.();
  }

  restoreFocus(): void {
    this.popToFocus("editor");
  }

  /**
   * Route a keyboard event through the focus tree.
   * Returns true if the event was handled.
   */
  handleKey(event: KeyEvent): boolean {
    // Emergency globals (Esc interrupt, Ctrl+D exit)
    if (this.globalHandler?.(event)) return true;

    const owner = this.owners.get(this.state.activeOwnerId);
    if (!owner) return false;

    // Try text handling for printable input
    if (event.char && event.char.length === 1 && event.char >= " " && owner.handleText) {
      if (owner.handleText(event.char)) return true;
    }

    // Run interceptors first (by priority)
    if (owner.interceptors) {
      const sorted = [...owner.interceptors].sort((a, b) => a.priority - b.priority);
      for (const interceptor of sorted) {
        const appState = this.stateAccessor?.();
        if (interceptor.match(event, appState)) {
          const result = interceptor.handle(event, appState);
          return this.processFocusResult(result);
        }
      }
    }

    // Fall back to owner's key handler
    if (owner.handleKey) {
      const result = owner.handleKey(event);
      return this.processFocusResult(result);
    }

    return false;
  }

  /**
   * Process a focus result from a key handler.
   */
  private processFocusResult(result: FocusResult): boolean {
    if ("handled" in result) {
      return result.handled;
    }
    if ("push" in result && result.push) {
      this.pushFocus(result.push.id, result.push.region, result.push.restoreTo);
      return true;
    }
    if ("pop" in result && result.pop) {
      this.popFocus();
      return true;
    }
    if ("popTo" in result && result.popTo) {
      this.popToFocus(result.popTo);
      return true;
    }
    return false;
  }

  isFocused(id: string): boolean {
    return this.state.activeOwnerId === id;
  }

  /**
   * Close a surface and all its descendants by popping focus back to before it.
   */
  closeSurface(surfaceId: string): void {
    const idx = this.state.stack.indexOf(surfaceId);
    if (idx < 0) return;
    this.popToFocus(this.state.stack[Math.max(0, idx - 1)]);
  }
}
