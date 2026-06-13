// ============================================================================
// ReadOnlyList — selectable list with keyboard navigation, no filter.
//
// Composes SelectListView + surface controller keyboard handling.
// Used for panels that just display a list: notifications, hotkeys, help,
// changelog, session-info, session-fork.
// ============================================================================

import { onCleanup, onMount } from "solid-js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { SelectListView } from "../select/SelectListView.js";
import type { SelectItem } from "../select/selector-controller.js";

export interface ReadOnlyListProps<T = unknown> {
  items: SelectItem<T>[];
  runtime: PanelRuntime;
  controller: TuiController;
  surfaceId: string;
  width: number;
  maxHeight?: number;
  itemSpacing?: number;
  onConfirm: (item: SelectItem<T>) => void | Promise<void>;
}

export function ReadOnlyList<T = unknown>(props: ReadOnlyListProps<T>) {
  const surface = () =>
    props.controller.store.state().surfaces.find((s) => s.id === props.surfaceId);
  const placement = () => surface()?.placement ?? "partial";
  const viewportHeight = () => props.controller.store.state().layout.viewport.height;

  const maxHeight = () => {
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 6);
    }
    let reserved = 0;
    const route = props.runtime.currentRoute;
    if (route.chrome.hints && route.chrome.hints.length > 0) {
      reserved += 1;
    }
    if (route.capabilities.some((c) => c.kind === "filter")) {
      reserved += 2;
    }
    return 12 - reserved;
  };

  onMount(() => {
    props.controller.setSurfaceController(props.surfaceId, {
      handleKey(event) {
        const current = props.runtime.state.selectedIndex ?? 0;
        let next = current;
        if (event.name === "up") next = Math.max(0, current - 1);
        else if (event.name === "down") next = Math.min(props.items.length - 1, current + 1);
        else if (event.name === "pageup") next = Math.max(0, current - 10);
        else if (event.name === "pagedown") next = Math.min(props.items.length - 1, current + 10);
        else if (event.name === "home") next = 0;
        else if (event.name === "end") next = Math.max(0, props.items.length - 1);

        if (next !== current) {
          props.runtime.dispatch({ type: "update_selection", index: next });
          return { type: "handled" };
        }
        if (event.name === "enter" || event.name === "return") {
          return { type: "confirm" };
        }
        return { type: "unhandled" };
      },
      async onConfirm() {
        const item = props.items[props.runtime.state.selectedIndex ?? 0];
        if (item) {
          await props.onConfirm(item);
        }
        props.runtime.dispatch({ type: "cancel" });
      },
    });
  });

  onCleanup(() => props.controller.setSurfaceController(props.surfaceId, null));

  return (
    <box flexDirection="column">
      <SelectListView
        items={props.items}
        selectedIndex={props.runtime.state.selectedIndex ?? 0}
        width={props.width}
        maxHeight={maxHeight()}
        showDescriptions
        itemSpacing={props.itemSpacing ?? 0}
        onSelect={() => {}}
      />
    </box>
  );
}
