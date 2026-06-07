// ============================================================================
// Theme module — public API
// ============================================================================

export { darkPalette, highContrastPalette, lightPalette } from "./palettes.js";
export { findPiThemes, loadPiThemeFile } from "./pi-theme-loader.js";
export {
  detectColorMode,
  getDefaultTheme,
  resolveTheme,
  setDefaultTheme,
} from "./resolve.js";
export type {
  DefaultPalette,
  HexColor,
  ResolvedTuiTheme,
  TuiBackgroundToken,
  TuiColorMode,
  TuiColorValue,
  TuiForegroundToken,
  TuiPalette,
  TuiThemeDefinition,
  TuiThemeTokens,
  TuiTokenPath,
} from "./schema.js";
export { buildDarkTokens } from "./themes/dark.js";
