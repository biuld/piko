// ============================================================================
// ListBody — scrollable selectable list.
//
// Thin wrapper over SelectListView: takes items + selection state,
// renders a scrollable list. No keyboard handling.
// ============================================================================

import type { SelectableListScrollPolicy } from "../../../surfaces/interactions/selectable-list.js";
import { SelectListView } from "../select/SelectListView.js";
import type { SelectItem } from "../select/selector-controller.js";

export interface ListBodyProps<T = unknown> {
  items: SelectItem<T>[];
  selectedIndex: number;
  maxHeight: number;
  width: number;
  showDescriptions?: boolean;
  itemSpacing?: number;
  scrollPolicy?: SelectableListScrollPolicy;
  rowHeight?: number;
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
      scrollPolicy={props.scrollPolicy}
      rowHeight={props.rowHeight}
      onSelect={() => {}}
    />
  );
}
