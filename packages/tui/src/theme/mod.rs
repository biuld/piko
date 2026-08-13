//! Theme system: typed semantic color slots + TOML loading.
//!
//! Architecture (Grok-style flat `Theme` slots + piko TOML authoring):
//! 1. [`Theme`] — every render-facing color is a named field ([`Theme::SLOT_COUNT`] slots).
//! 2. Installed themes live in `$PIKO_HOME/themes/` (default `~/.piko/themes/`).
//! 3. Missing slots on custom files fall back to the installed `dark` theme.
//! 4. Slot keys are snake_case only (matching `Theme` field names).

mod resolve;
mod slots;

#[cfg(test)]
mod tests;

pub use slots::Theme;

use std::collections::HashMap;
use std::path::PathBuf;

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

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ThemeError {
    Io(String),
    Parse(String),
    InvalidName(String),
    MissingVar(String),
    CircularVar(String),
    InvalidHex(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "Failed to load theme: {msg}"),
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
    /// Load the installed dark theme.
    pub fn dark() -> Self {
        Self::load_required("dark")
    }

    /// Load the installed light theme.
    pub fn light() -> Self {
        Self::load_required("light")
    }

    /// Resolve a theme by name; an unknown or invalid selection uses dark.
    pub fn load(name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        if name == "dark" {
            return Self::dark();
        }
        if name == "light" {
            return Self::light();
        }
        match read_theme(&name).and_then(|content| Self::from_toml_str(&content)) {
            Ok(theme) => theme,
            Err(error) => {
                eprintln!("piko: {error}; using dark theme");
                Self::dark()
            }
        }
    }

    fn load_required(name: &str) -> Self {
        let content = read_theme(name).unwrap_or_else(|error| {
            panic!("{error}; run scripts/install.sh to initialize the piko installation")
        });
        let raw: ThemeToml = toml::from_str(&content)
            .unwrap_or_else(|error| panic!("invalid installed {name} theme: {error}"));
        let vars = resolve::resolve_vars(&raw.vars)
            .unwrap_or_else(|error| panic!("invalid installed {name} theme: {error}"));
        let resolved = resolve::resolve_colors(&raw.colors, &vars)
            .unwrap_or_else(|error| panic!("invalid installed {name} theme: {error}"));
        if name == "dark" {
            resolve::theme_from_complete_map(raw.theme.name, &resolved)
                .unwrap_or_else(|error| panic!("invalid installed dark theme: {error}"))
        } else {
            Self::with_overrides(&Self::dark(), raw.theme.name, &resolved)
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

fn piko_dir() -> PathBuf {
    if let Some(root) = std::env::var_os("PIKO_HOME") {
        return PathBuf::from(root);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".piko")
}

fn read_theme(name: &str) -> Result<String, ThemeError> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == ".." {
        return Err(ThemeError::InvalidName(name.to_string()));
    }
    let filename = format!("{name}.toml");
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".piko/themes").join(&filename));
    }
    #[cfg(test)]
    candidates.push(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/themes")
            .join(&filename),
    );
    candidates.push(piko_dir().join("themes").join(&filename));
    #[cfg(all(debug_assertions, not(test)))]
    candidates.push(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/themes")
            .join(&filename),
    );

    for path in candidates {
        match std::fs::read_to_string(&path) {
            Ok(content) => return Ok(content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ThemeError::Io(format!("{}: {error}", path.display())));
            }
        }
    }
    Err(ThemeError::Io(format!("theme '{name}' was not found")))
}
