//! Theme system: TOML-based semantic color tokens.
//!
//! Built-in themes are embedded via `include_str!` from `resources/themes/`.
//! Custom themes are loaded from `~/.piko/themes/` and `.piko/themes/`.
//!
//! Architecture:
//! 1. ThemeToml deserialized from TOML
//! 2. Var references resolved ([vars] → [colors])
//! 3. Color values converted to ratatui `Color`
//! 4. Missing tokens filled from built-in dark defaults

use std::collections::HashMap;

use ratatui::style::Color;
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

// ── Theme ────────────────────────────────────────────────────────────────────

/// Resolved theme: all Layer-1 tokens are ratatui `Color` values, ready for
/// direct use in rendering.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,

    // ── Layer 1: Core UI (actively used in rendering) ──
    pub text: Color,
    pub dim: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub border: Color,
    pub border_accent: Color,
    pub border_muted: Color,

    // All resolved tokens (Layer 1 + Layer 2 + Layer 3), keyed by token name.
    // Layer-1 fields above are convenience accessors into this map.
    all: HashMap<String, Color>,
}

mod resolve;
#[cfg(test)]
mod tests;

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

// ── Tests ────────────────────────────────────────────────────────────────────
