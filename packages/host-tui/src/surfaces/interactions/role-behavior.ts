// ============================================================================
// Role behavior handlers for the Surface UX Contract
// ============================================================================

import type { KeyEvent } from "../../focus/types.js";
import { handleSelectableListKey, type SelectableListState } from "./selectable-list.js";

export type SurfaceKeyResult =
  | { type: "handled" }
  | { type: "close" }
  | { type: "confirm"; value?: unknown }
  | { type: "submit"; value?: unknown }
  | { type: "unhandled" };

export function selectorBehavior(
  event: KeyEvent,
  state: SelectableListState,
  total: number,
): { nextState: SelectableListState; result: SurfaceKeyResult } {
  const next = handleSelectableListKey(state, event, {
    total,
    filterable: true,
  });
  if (next) {
    return { nextState: next, result: { type: "handled" } };
  }
  if (event.name === "enter" || event.name === "return") {
    return { nextState: state, result: { type: "confirm" } };
  }
  if (event.name === "escape") {
    return { nextState: state, result: { type: "close" } };
  }
  return { nextState: state, result: { type: "unhandled" } };
}

export function menuBehavior(
  event: KeyEvent,
  state: SelectableListState,
  total: number,
): { nextState: SelectableListState; result: SurfaceKeyResult } {
  const next = handleSelectableListKey(state, event, {
    total,
    filterable: true,
  });
  if (next) {
    return { nextState: next, result: { type: "handled" } };
  }
  if (event.name === "enter" || event.name === "return") {
    return { nextState: state, result: { type: "confirm" } };
  }
  if (event.name === "escape") {
    return { nextState: state, result: { type: "close" } };
  }
  return { nextState: state, result: { type: "unhandled" } };
}

export interface FormState {
  value: string;
}

export function formBehavior(
  event: KeyEvent,
  state: FormState,
): { nextState: FormState; result: SurfaceKeyResult } {
  if (event.char && event.char.length === 1 && event.char >= " ") {
    return {
      nextState: { value: state.value + event.char },
      result: { type: "handled" },
    };
  }
  if (event.name === "backspace") {
    return {
      nextState: { value: state.value.slice(0, -1) },
      result: { type: "handled" },
    };
  }
  if (event.name === "enter" || event.name === "return") {
    return { nextState: state, result: { type: "submit", value: state.value } };
  }
  if (event.name === "escape") {
    return { nextState: state, result: { type: "close" } };
  }
  return { nextState: state, result: { type: "unhandled" } };
}

export interface ConfirmState {
  activeOption: "confirm" | "cancel";
}

export function confirmBehavior(
  event: KeyEvent,
  state: ConfirmState,
): { nextState: ConfirmState; result: SurfaceKeyResult } {
  if (event.name === "left" || event.name === "right" || event.name === "tab") {
    const nextOption = state.activeOption === "confirm" ? "cancel" : "confirm";
    return {
      nextState: { activeOption: nextOption },
      result: { type: "handled" },
    };
  }
  if (event.name === "enter" || event.name === "return") {
    if (state.activeOption === "confirm") {
      return { nextState: state, result: { type: "confirm" } };
    } else {
      return { nextState: state, result: { type: "close" } };
    }
  }
  if (event.name === "escape") {
    return { nextState: state, result: { type: "close" } };
  }
  return { nextState: state, result: { type: "unhandled" } };
}
