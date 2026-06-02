// ============================================================================
// Selector layout — compute selector dimensions
// ============================================================================

import type { SelectorLayout } from "./selector-controller.js";

/**
 * Compute selector layout based on available height and item count.
 */
export function computeSelectorLayout(
  totalItems: number,
  availableHeight: number,
  hasFilter: boolean,
  terminalWidth: number,
): SelectorLayout {
  const fixedRows =
    1 + // title
    (hasFilter ? 1 : 0) + // filter row
    1; // hint row

  const maxListRows = Math.max(3, Math.min(12, availableHeight - fixedRows));
  const visibleListRows = Math.min(maxListRows, totalItems);
  const showDescriptions = terminalWidth >= 60;
  const showScrollCounter = totalItems > visibleListRows;

  return {
    maxListRows,
    visibleListRows,
    totalItems,
    showFilter: hasFilter,
    showDescriptions,
    showScrollCounter,
  };
}
