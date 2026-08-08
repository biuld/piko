//! Theme system: typed semantic color slots + TOML loading.
//!
//! Architecture (Grok-style flat `Theme` slots + piko TOML authoring):
//! 1. [`Theme`] — every render-facing color is a named field ([`Theme::SLOT_COUNT`] slots).
//! 2. Built-in themes ship as TOML; custom themes live in `~/.piko/themes/`.
//! 3. Missing slots on custom files fall back to built-in `dark`.
//! 4. Legacy camelCase keys are accepted via slot aliases.

mod resolve;
mod slots;

#[cfg(test)]
mod tests;

pub use slots::Theme;

use std::collections::HashMap;

use serde::Deserialize;

// ── TOML shapes ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ThemeToml {
    theme: ThemeHeader,
    #[serde(default)]
    vars: HashMap<String, ColorValue>,
    #[serde(default)]
    colors: HashMap<String, ColorValue>,
}

#[derive(Debug, Deserialize)]
struct ThemeHeader {
    name: String,
}

/// A color value as it appears in TOML.
/// - Integer → 256-color palette index (0–255)
/// - String starting with `#` → hex RGB
/// - String (other) → variable reference to `[vars]`
/// - Empty string → terminal default
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum ColorValue {
    Index(u8),
    Text(String),
}

// ── Embedded built-in themes ─────────────────────────────────────────────────

const DARK_TOML: &str = include_str!("../../resources/themes/dark.toml");
const LIGHT_TOML: &str = include_str!("../../resources/themes/light.toml");

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ThemeError {
    Parse(String),
    InvalidName(String),
    MissingVar(String),
    CircularVar(String),
    InvalidHex(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "Invalid theme TOML: {msg}"),
            Self::InvalidName(name) => {
                write!(f, "Invalid theme name '{name}': must not contain '/'")
            }
            Self::MissingVar(key) => write!(f, "Variable '{key}' not found in [vars]"),
            Self::CircularVar(key) => write!(f, "Circular variable reference detected: '{key}'"),
            Self::InvalidHex(hex) => write!(f, "Invalid hex color: '{hex}'"),
        }
    }
}

impl std::error::Error for ThemeError {}

// ── Public constructors (on Theme) ───────────────────────────────────────────

impl Theme {
    /// Load the built-in dark theme.
    pub fn dark() -> Self {
        Self::from_toml_str(DARK_TOML).expect("built-in dark.toml must be valid")
    }

    /// Load the built-in light theme.
    pub fn light() -> Self {
        Self::from_toml_str(LIGHT_TOML).expect("built-in light.toml must be valid")
    }

    /// Resolve a theme by name (built-ins today; unknown → dark).
    pub fn load(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "light" => Self::light(),
            // "dark" and anything unknown fall back to the complete dark catalog.
            _ => Self::dark(),
        }
    }

    /// Parse and resolve a TOML theme string.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ThemeError> {
        let raw: ThemeToml =
            toml::from_str(toml_str).map_err(|e| ThemeError::Parse(e.to_string()))?;

        if raw.theme.name.contains('/') {
            return Err(ThemeError::InvalidName(raw.theme.name));
        }

        let vars = resolve::resolve_vars(&raw.vars)?;
        let resolved = resolve::resolve_colors(&raw.colors, &vars)?;

        // Built-in dark is the complete baseline; never recurse through dark()
        // when parsing dark itself (would loop). Completeness is guaranteed
        // by resources/themes/dark.toml asserting all slots.
        if raw.theme.name == "dark" {
            Ok(resolve::theme_from_complete_map(raw.theme.name, &resolved)?)
        } else {
            let base = Self::dark();
            Ok(Self::with_overrides(&base, raw.theme.name, &resolved))
        }
    }
}
