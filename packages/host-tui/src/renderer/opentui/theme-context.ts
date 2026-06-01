// ============================================================================
// Theme context — provides resolved theme to all OpenTUI components
// ============================================================================

import { createContext, useContext } from "solid-js";
import { getDefaultTheme } from "../../theme/resolve.js";
import type { ResolvedTuiTheme } from "../../theme/schema.js";

// ============================================================================
// Context
// ============================================================================

const ThemeContext = createContext<ResolvedTuiTheme>(getDefaultTheme());

export const ThemeProvider = ThemeContext.Provider;

/**
 * Hook to access the current resolved theme from any component.
 */
export function useTheme(): ResolvedTuiTheme {
  return useContext(ThemeContext) ?? getDefaultTheme();
}
