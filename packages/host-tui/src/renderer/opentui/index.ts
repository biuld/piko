// ============================================================================
// OpenTUI Renderer — public entry point
// ============================================================================

// Re-export controller
export { TuiController } from "../../runtime/tui-controller.js";
export type { AppProps } from "./App.js";
export { App } from "./App.js";
// Autocomplete
export { CommandAutocomplete } from "./autocomplete/CommandAutocomplete.js";
// Hints
export { HintLine } from "./hints/HintLine.js";
export { KeyHint } from "./hints/KeyHint.js";
// Select components
export { SelectListView } from "./select/SelectListView.js";
export { SelectorShell } from "./select/SelectorShell.js";
export type { TuiStore } from "./store.js";
export { createDefaultStore, createTuiStore } from "./store.js";

// Surface hosts
export { SurfaceHost } from "./surfaces/SurfaceHost.js";
export { TimelineItemView } from "./timeline/TimelineItemView.js";
// Timeline components
export { TimelineView } from "./timeline/TimelineView.js";
