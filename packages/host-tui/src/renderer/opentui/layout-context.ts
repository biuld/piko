// ============================================================================
// Layout context — provides layout state to all OpenTUI components
// ============================================================================

import { createContext, useContext } from "solid-js";
import type { TuiLayoutState } from "../../state/state.js";

const LayoutContext = createContext<TuiLayoutState>({
  viewport: { width: 80, height: 24 },
  mode: "regular",
  activeRegion: "editor",
  bottomBar: {
    density: "full",
    visibleFields: ["model", "session", "branch", "tokens", "cost", "cwd", "mode", "hints"],
  },
  theme: "dark",
  hideThinking: false,
});

export const LayoutProvider = LayoutContext.Provider;

/**
 * Hook to access the current layout from any component.
 */
export function useLayout(): TuiLayoutState {
  return useContext(LayoutContext);
}
