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

const DARK_TOML: &str = include_str!("../resources/themes/dark.toml");

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

impl Theme {
    // ── built-in constructors ─────────────────────────────────────────────────

    /// Load the built-in dark theme.
    pub fn dark() -> Self {
        Self::from_toml_str(DARK_TOML).expect("built-in dark.toml must be valid")
    }

    /// Parse and resolve a TOML string.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, ThemeError> {
        let raw: ThemeToml =
            toml::from_str(toml_str).map_err(|e| ThemeError::Parse(e.to_string()))?;

        // Validate name: must not contain '/'
        if raw.theme.name.contains('/') {
            return Err(ThemeError::InvalidName(raw.theme.name));
        }

        // Resolve [vars] → flat Color map
        let vars = resolve_vars(&raw.vars)?;

        // Resolve [colors] → flat Color map
        let mut resolved = resolve_colors(&raw.colors, &vars)?;

        // Fill missing tokens from built-in dark defaults
        let dark_defaults = dark_color_map();
        for (key, color) in &dark_defaults {
            resolved.entry(key.clone()).or_insert(*color);
        }

        Ok(Self::from_resolved(raw.theme.name, &resolved))
    }

    /// Build a Theme from a resolved color map.
    fn from_resolved(name: String, colors: &HashMap<String, Color>) -> Self {
        Self {
            name,
            text: get_color(colors, "text"),
            dim: get_color(colors, "dim"),
            muted: get_color(colors, "muted"),
            accent: get_color(colors, "accent"),
            accent_alt: get_color(colors, "accentAlt"),
            success: get_color(colors, "success"),
            error: get_color(colors, "error"),
            warning: get_color(colors, "warning"),
            info: get_color(colors, "info"),
            border: get_color(colors, "border"),
            border_accent: get_color(colors, "borderAccent"),
            border_muted: get_color(colors, "borderMuted"),
            all: colors.clone(),
        }
    }

    /// Look up an arbitrary token by name (for Layer 2/3).
    pub fn get(&self, key: &str) -> Color {
        self.all.get(key).copied().unwrap_or(Color::Reset)
    }
}

// ── Resolution ───────────────────────────────────────────────────────────────

#[allow(clippy::ptr_arg)]
fn resolve_vars(vars: &HashMap<String, ColorValue>) -> Result<HashMap<String, Color>, ThemeError> {
    let mut resolved: HashMap<String, Color> = HashMap::new();
    let mut resolving = Vec::new();

    for key in vars.keys() {
        resolve_var(key, vars, &mut resolved, &mut resolving)?;
    }
    Ok(resolved)
}

#[allow(clippy::ptr_arg)]
fn resolve_var(
    key: &str,
    vars: &HashMap<String, ColorValue>,
    resolved: &mut HashMap<String, Color>,
    resolving: &mut Vec<String>,
) -> Result<Color, ThemeError> {
    if let Some(&color) = resolved.get(key) {
        return Ok(color);
    }
    if resolving.iter().any(|k| k == key) {
        return Err(ThemeError::CircularVar(key.to_string()));
    }

    let value = vars
        .get(key)
        .ok_or_else(|| ThemeError::MissingVar(key.to_string()))?;

    resolving.push(key.to_string());
    let color = match value {
        ColorValue::Index(n) => Color::Indexed(*n),
        ColorValue::Text(s) if s.is_empty() => Color::Reset,
        ColorValue::Text(s) if s.starts_with('#') => parse_hex(s)?,
        ColorValue::Text(s) => resolve_var(s, vars, resolved, resolving)?,
    };
    resolving.pop();

    resolved.insert(key.to_string(), color);
    Ok(color)
}

fn resolve_colors(
    colors: &HashMap<String, ColorValue>,
    vars: &HashMap<String, Color>,
) -> Result<HashMap<String, Color>, ThemeError> {
    let mut resolved: HashMap<String, Color> = HashMap::new();
    let mut resolving = Vec::new();

    for (key, value) in colors {
        let color = resolve_color_value(value, vars, &mut resolving)?;
        resolved.insert(key.clone(), color);
    }
    Ok(resolved)
}

#[allow(clippy::ptr_arg)]
fn resolve_color_value(
    value: &ColorValue,
    vars: &HashMap<String, Color>,
    resolving: &mut Vec<String>,
) -> Result<Color, ThemeError> {
    match value {
        ColorValue::Index(n) => Ok(Color::Indexed(*n)),
        ColorValue::Text(s) if s.is_empty() => Ok(Color::Reset),
        ColorValue::Text(s) if s.starts_with('#') => parse_hex(s),
        ColorValue::Text(s) => {
            if resolving.iter().any(|k| k == s) {
                return Err(ThemeError::CircularVar(s.clone()));
            }
            vars.get(s)
                .copied()
                .ok_or_else(|| ThemeError::MissingVar(s.clone()))
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse_hex(hex: &str) -> Result<Color, ThemeError> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(ThemeError::InvalidHex(hex.to_string()));
    }
    let r =
        u8::from_str_radix(&hex[0..2], 16).map_err(|_| ThemeError::InvalidHex(hex.to_string()))?;
    let g =
        u8::from_str_radix(&hex[2..4], 16).map_err(|_| ThemeError::InvalidHex(hex.to_string()))?;
    let b =
        u8::from_str_radix(&hex[4..6], 16).map_err(|_| ThemeError::InvalidHex(hex.to_string()))?;
    Ok(Color::Rgb(r, g, b))
}

fn get_color(colors: &HashMap<String, Color>, key: &str) -> Color {
    colors.get(key).copied().unwrap_or(Color::Reset)
}

/// Build a fallback color map from the built-in dark theme.
/// Used to fill missing tokens when loading custom themes.
fn dark_color_map() -> HashMap<String, Color> {
    // Parse dark theme once and cache the resolved colors.
    // Inline the defaults so we avoid a circular dependency at compile time.
    let vars = dark_default_vars();
    let colors = dark_default_colors();
    let mut resolved: HashMap<String, Color> = HashMap::new();

    // Resolve each color through vars
    for (key, value) in &colors {
        let color = color_value_to_color(value, &vars);
        resolved.insert(key.clone(), color);
    }
    resolved
}

fn dark_default_vars() -> HashMap<String, Color> {
    let mut m = HashMap::new();
    m.insert("cyan".to_string(), Color::Rgb(0, 215, 255));
    m.insert("blue".to_string(), Color::Rgb(95, 135, 255));
    m.insert("green".to_string(), Color::Rgb(181, 189, 104));
    m.insert("red".to_string(), Color::Rgb(204, 102, 102));
    m.insert("yellow".to_string(), Color::Rgb(255, 255, 0));
    m.insert("text_color".to_string(), Color::Rgb(212, 212, 212));
    m.insert("gray".to_string(), Color::Rgb(128, 128, 128));
    m.insert("dim_gray".to_string(), Color::Rgb(102, 102, 102));
    m.insert("dark_gray".to_string(), Color::Rgb(80, 80, 80));
    m.insert("accent_color".to_string(), Color::Rgb(138, 190, 183));
    m.insert("selected_bg".to_string(), Color::Rgb(58, 58, 74));
    m.insert("user_msg_bg".to_string(), Color::Rgb(52, 53, 65));
    m.insert("tool_pending_bg".to_string(), Color::Rgb(40, 40, 50));
    m.insert("tool_success_bg".to_string(), Color::Rgb(40, 50, 40));
    m.insert("tool_error_bg".to_string(), Color::Rgb(60, 40, 40));
    m.insert("custom_msg_bg".to_string(), Color::Rgb(45, 40, 56));
    m
}

fn dark_default_colors() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("accent".to_string(), "accent_color".to_string());
    m.insert("accentAlt".to_string(), "blue".to_string());
    m.insert("border".to_string(), "blue".to_string());
    m.insert("borderAccent".to_string(), "cyan".to_string());
    m.insert("borderMuted".to_string(), "dark_gray".to_string());
    m.insert("success".to_string(), "green".to_string());
    m.insert("error".to_string(), "red".to_string());
    m.insert("warning".to_string(), "yellow".to_string());
    m.insert("info".to_string(), "blue".to_string());
    m.insert("muted".to_string(), "gray".to_string());
    m.insert("dim".to_string(), "dim_gray".to_string());
    m.insert("text".to_string(), "text_color".to_string());
    m.insert("thinkingText".to_string(), "gray".to_string());
    m.insert("selectedBg".to_string(), "selected_bg".to_string());
    m.insert("userMessageBg".to_string(), "user_msg_bg".to_string());
    m.insert("userMessageText".to_string(), "text_color".to_string());
    m.insert("customMessageBg".to_string(), "custom_msg_bg".to_string());
    m.insert("customMessageText".to_string(), "text_color".to_string());
    m.insert("customMessageLabel".to_string(), "#9575cd".to_string());
    m.insert("toolPendingBg".to_string(), "tool_pending_bg".to_string());
    m.insert("toolSuccessBg".to_string(), "tool_success_bg".to_string());
    m.insert("toolErrorBg".to_string(), "tool_error_bg".to_string());
    m.insert("toolTitle".to_string(), "text_color".to_string());
    m.insert("toolOutput".to_string(), "gray".to_string());
    m.insert("mdHeading".to_string(), "#f0c674".to_string());
    m.insert("mdLink".to_string(), "#81a2be".to_string());
    m.insert("mdLinkUrl".to_string(), "dim_gray".to_string());
    m.insert("mdCode".to_string(), "accent_color".to_string());
    m.insert("mdCodeBlock".to_string(), "green".to_string());
    m.insert("mdCodeBlockBorder".to_string(), "gray".to_string());
    m.insert("mdQuote".to_string(), "gray".to_string());
    m.insert("mdQuoteBorder".to_string(), "gray".to_string());
    m.insert("mdHr".to_string(), "gray".to_string());
    m.insert("mdListBullet".to_string(), "accent_color".to_string());
    m.insert("toolDiffAdded".to_string(), "green".to_string());
    m.insert("toolDiffRemoved".to_string(), "red".to_string());
    m.insert("toolDiffContext".to_string(), "gray".to_string());
    m.insert("syntaxComment".to_string(), "#6A9955".to_string());
    m.insert("syntaxKeyword".to_string(), "#569CD6".to_string());
    m.insert("syntaxFunction".to_string(), "#DCDCAA".to_string());
    m.insert("syntaxVariable".to_string(), "#9CDCFE".to_string());
    m.insert("syntaxString".to_string(), "#CE9178".to_string());
    m.insert("syntaxNumber".to_string(), "#B5CEA8".to_string());
    m.insert("syntaxType".to_string(), "#4EC9B0".to_string());
    m.insert("syntaxOperator".to_string(), "#D4D4D4".to_string());
    m.insert("syntaxPunctuation".to_string(), "#D4D4D4".to_string());
    m.insert("thinkingOff".to_string(), "dark_gray".to_string());
    m.insert("thinkingMinimal".to_string(), "#6e6e6e".to_string());
    m.insert("thinkingLow".to_string(), "#5f87af".to_string());
    m.insert("thinkingMedium".to_string(), "#81a2be".to_string());
    m.insert("thinkingHigh".to_string(), "#b294bb".to_string());
    m.insert("thinkingXhigh".to_string(), "#d183e8".to_string());
    m.insert("bashMode".to_string(), "green".to_string());
    m
}

fn color_value_to_color(value: &str, vars: &HashMap<String, Color>) -> Color {
    if value.is_empty() {
        Color::Reset
    } else if value.starts_with('#') {
        parse_hex(value).unwrap_or(Color::Reset)
    } else if let Some(&color) = vars.get(value) {
        color
    } else {
        // Try parsing as hex (in case it's a direct hex in colors)
        if value.starts_with('#') {
            parse_hex(value).unwrap_or(Color::Reset)
        } else {
            Color::Reset
        }
    }
}

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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_loads() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "dark");
        // Layer 1 tokens should be non-Reset
        assert_ne!(theme.accent, Color::Reset);
        assert_ne!(theme.text, Color::Reset);
        assert_ne!(theme.border, Color::Reset);
        assert_ne!(theme.border_muted, Color::Reset);
    }

    #[test]
    fn test_light_theme_loads() {
        let light = include_str!("../resources/themes/light.toml");
        let theme = Theme::from_toml_str(light).expect("built-in light.toml must be valid");
        assert_eq!(theme.name, "light");
        assert_ne!(theme.accent, Color::Reset);
    }

    #[test]
    fn test_all_tokens_present() {
        let theme = Theme::dark();
        // All 40+ tokens should be resolvable via get()
        for key in dark_default_colors().keys() {
            let color = theme.get(key);
            assert!(
                color != Color::Reset || key == "text",
                "Token '{key}' should not be Reset"
            );
        }
    }

    #[test]
    fn test_reject_slash_in_name() {
        let toml = r#"
            [theme]
            name = "bad/name"
            [colors]
            text = ""
        "#;
        let err = Theme::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ThemeError::InvalidName(_)));
    }

    #[test]
    fn test_var_resolution() {
        let toml = r##"
            [theme]
            name = "test"
            [vars]
            my_blue = "#0000ff"
            [colors]
            accent = "my_blue"
            text = ""
        "##;
        let theme = Theme::from_toml_str(toml).unwrap();
        assert_eq!(theme.accent, Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_256_color_index() {
        let toml = r#"
            [theme]
            name = "test"
            [colors]
            accent = 196
            text = ""
        "#;
        let theme = Theme::from_toml_str(toml).unwrap();
        assert_eq!(theme.accent, Color::Indexed(196));
    }

    #[test]
    fn test_direct_hex_in_colors() {
        let toml = r##"
            [theme]
            name = "test"
            [colors]
            accent = "#ff00ff"
            text = ""
        "##;
        let theme = Theme::from_toml_str(toml).unwrap();
        assert_eq!(theme.accent, Color::Rgb(255, 0, 255));
    }

    #[test]
    fn test_empty_text_is_reset() {
        let toml = r#"
            [theme]
            name = "test"
            [colors]
            text = ""
        "#;
        let theme = Theme::from_toml_str(toml).unwrap();
        assert_eq!(theme.text, Color::Reset);
    }

    #[test]
    fn test_missing_tokens_fall_back_to_dark() {
        let toml = r##"
            [theme]
            name = "minimal"
            [colors]
            accent = "#ff0000"
            text = ""
        "##;
        let theme = Theme::from_toml_str(toml).unwrap();
        // accent is set, others fall back to dark
        assert_eq!(theme.accent, Color::Rgb(255, 0, 0));
        assert_ne!(theme.border, Color::Reset); // from dark defaults
        assert_ne!(theme.dim, Color::Reset); // from dark defaults
    }

    #[test]
    fn test_circular_var_detected() {
        let toml = r#"
            [theme]
            name = "test"
            [vars]
            a = "b"
            b = "a"
            [colors]
            accent = "a"
            text = ""
        "#;
        let err = Theme::from_toml_str(toml).unwrap_err();
        assert!(matches!(err, ThemeError::CircularVar(_)));
    }
}
