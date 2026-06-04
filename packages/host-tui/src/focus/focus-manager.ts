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

  /** Listener called on focus state change */
  private onChangeListener?: (state: TuiFocusState) => void;

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

  /** Register a listener for focus state changes (for store sync). */
  onChange(listener: (state: TuiFocusState) => void): void {
    this.onChangeListener = listener;
  }

  getState(): TuiFocusState {
    return { ...this.state, path: [...this.state.path], stack: [...this.state.stack] };
  }

  pushFocus(id: string, region: FocusRegion, restoreTo?: string): void {
    const prevId = this.state.activeOwnerId;
    const prevOwner = this.owners.get(prevId);
    prevOwner?.blur?.();

    // Record restore target for this focus push
    const effectiveRestoreTo = restoreTo ?? prevId;

    this.state.stack.push(id);
    this.state.path.push(id);
    this.state.activeOwnerId = id;
    this.state.region = region;

    // Store restoreTo on the owner for later use
    const owner = this.owners.get(id);
    if (owner) {
      (owner as any)._restoreTo = effectiveRestoreTo;
      owner.focus?.();
    }

    this.emitChange();
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

    this.emitChange();
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

    this.emitChange();
  }

  restoreFocus(): void {
    this.popToFocus("editor");
  }

  /**
   * Route a keyboard event through the focus tree with parent bubbling.
   * Returns true if the event was handled.
   *
   * Order:
   * 1. Global handler (Esc interrupt, etc.)
   * 2. Active owner's interceptors (by priority)
   * 3. Active owner's handleKey
   * 4. If not handled, bubble to parent owner (previous in stack)
   */
  handleKey(event: KeyEvent): boolean {
    // Emergency globals (Esc interrupt, Ctrl+D exit)
    if (this.globalHandler?.(event)) return true;

    // Try from the deepest active owner, bubbling up to parents
    for (let i = this.state.stack.length - 1; i >= 0; i--) {
      const ownerId = this.state.stack[i];
      const owner = this.owners.get(ownerId);
      if (!owner) continue;

      // Try text handling for printable input (only at the active level)
      if (
        i === this.state.stack.length - 1 &&
        event.char &&
        event.char.length === 1 &&
        event.char >= " " &&
        owner.handleText
      ) {
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
        if ("handled" in result && result.handled) {
          return this.processFocusResult(result);
        }
        if ("handled" in result && !result.handled) {
          // Bubble up to parent
          continue;
        }
        // Push/pop/popTo results are always handled
        return this.processFocusResult(result);
      }
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

  private emitChange(): void {
    this.onChangeListener?.(this.getState());
  }

  /**
   * Close a surface and all its descendants by popping focus back to restore target.
   */
  closeSurface(surfaceId: string): void {
    const idx = this.state.stack.indexOf(surfaceId);
    if (idx < 0) return;

    // Find the restore target for this surface, or fall back to the previous element
    const owner = this.owners.get(surfaceId);
    const restoreTo = (owner as any)?._restoreTo ?? this.state.stack[Math.max(0, idx - 1)];
    this.popToFocus(restoreTo);
  }
}
