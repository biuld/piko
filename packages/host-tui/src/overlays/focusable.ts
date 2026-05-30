import type { Container, Focusable } from "@earendil-works/pi-tui";

/** Create a Container + Focusable by adding a focused getter/setter */
export function makeFocusable(
  container: Container,
  focusedChild?: { focused: boolean },
): Container & Focusable {
  let _focused = false;
  Object.defineProperty(container, "focused", {
    get() {
      return _focused;
    },
    set(v: boolean) {
      _focused = v;
      if (focusedChild) focusedChild.focused = v;
    },
    enumerable: true,
    configurable: true,
  });
  return container as Container & Focusable;
}
