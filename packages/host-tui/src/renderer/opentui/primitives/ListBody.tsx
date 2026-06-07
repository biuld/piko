// ============================================================================
// ListBody — scrollable selectable list.
//
// Thin wrapper over SelectListView: takes items + selection state,
// renders a scrollable list. No keyboard handling.
// ============================================================================

import type { SelectItem } from "../select/selector-controller.js";
import { SelectListView } from "../select/SelectListView.js";

export interface ListBodyProps<T = unknown> {
  items: SelectItem<T>[];
  selectedIndex: number;
  maxHeight: number;
  width: number;
  showDescriptions?: boolean;
  itemSpacing?: number;
}

export function ListBody<T = unknown>(props: ListBodyProps<T>) {
  return (
    <SelectListView
      items={props.items}
      selectedIndex={props.selectedIndex}
      width={props.width}
      maxHeight={props.maxHeight}
      showDescriptions={props.showDescriptions ?? false}
      itemSpacing={props.itemSpacing ?? 0}
      onSelect={() => {}}
    />
  );
}
