use super::*;

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
    pub(super) fn from_resolved(name: String, colors: &HashMap<String, Color>) -> Self {
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
pub(super) fn resolve_vars(
    vars: &HashMap<String, ColorValue>,
) -> Result<HashMap<String, Color>, ThemeError> {
    let mut resolved: HashMap<String, Color> = HashMap::new();
    let mut resolving = Vec::new();

    for key in vars.keys() {
        resolve_var(key, vars, &mut resolved, &mut resolving)?;
    }
    Ok(resolved)
}

#[allow(clippy::ptr_arg)]
pub(super) fn resolve_var(
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

pub(super) fn resolve_colors(
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
pub(super) fn resolve_color_value(
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

pub(super) fn parse_hex(hex: &str) -> Result<Color, ThemeError> {
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

pub(super) fn get_color(colors: &HashMap<String, Color>, key: &str) -> Color {
    colors.get(key).copied().unwrap_or(Color::Reset)
}

/// Build a fallback color map from the built-in dark theme.
/// Used to fill missing tokens when loading custom themes.
pub(super) fn dark_color_map() -> HashMap<String, Color> {
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

pub(super) fn dark_default_vars() -> HashMap<String, Color> {
    // Mirror resources/themes/dark.toml [vars] for fill-missing-token fallbacks.
    let mut m = HashMap::new();
    m.insert("accent_blue".to_string(), Color::Rgb(95, 135, 255));
    m.insert("cyan".to_string(), Color::Rgb(0, 215, 255));
    m.insert("green".to_string(), Color::Rgb(181, 189, 104));
    m.insert("red".to_string(), Color::Rgb(204, 102, 102));
    m.insert("yellow".to_string(), Color::Rgb(255, 255, 0));
    m.insert("text_color".to_string(), Color::Rgb(212, 212, 212));
    m.insert("gray".to_string(), Color::Rgb(128, 128, 128));
    m.insert("dim_gray".to_string(), Color::Rgb(102, 102, 102));
    m.insert("dark_gray".to_string(), Color::Rgb(80, 80, 80));
    m.insert("selected_bg".to_string(), Color::Rgb(58, 58, 74));
    m.insert("user_msg_bg".to_string(), Color::Rgb(52, 53, 65));
    m.insert("tool_pending_bg".to_string(), Color::Rgb(40, 40, 50));
    m.insert("tool_success_bg".to_string(), Color::Rgb(40, 50, 40));
    m.insert("tool_error_bg".to_string(), Color::Rgb(60, 40, 40));
    m.insert("custom_msg_bg".to_string(), Color::Rgb(45, 40, 56));
    m.insert("purple".to_string(), Color::Rgb(149, 117, 205));
    m.insert("gold".to_string(), Color::Rgb(240, 198, 116));
    m.insert("sky".to_string(), Color::Rgb(129, 162, 190));
    m.insert("syntax_comment".to_string(), Color::Rgb(106, 153, 85));
    m.insert("syntax_keyword".to_string(), Color::Rgb(86, 156, 214));
    m.insert("syntax_function".to_string(), Color::Rgb(220, 220, 170));
    m.insert("syntax_variable".to_string(), Color::Rgb(156, 220, 254));
    m.insert("syntax_string".to_string(), Color::Rgb(206, 145, 120));
    m.insert("syntax_number".to_string(), Color::Rgb(181, 206, 168));
    m.insert("syntax_type".to_string(), Color::Rgb(78, 201, 176));
    m.insert("thinking_minimal".to_string(), Color::Rgb(110, 110, 110));
    m.insert("thinking_low".to_string(), Color::Rgb(95, 135, 175));
    m.insert("thinking_medium".to_string(), Color::Rgb(129, 162, 190));
    m.insert("thinking_high".to_string(), Color::Rgb(178, 148, 187));
    m.insert("thinking_xhigh".to_string(), Color::Rgb(209, 131, 232));
    m
}

pub(super) fn dark_default_colors() -> HashMap<String, String> {
    // Mirror resources/themes/dark.toml [colors]. Focused chrome uses `accent`.
    let mut m = HashMap::new();
    m.insert("accent".to_string(), "accent_blue".to_string());
    m.insert("accentAlt".to_string(), "cyan".to_string());
    m.insert("border".to_string(), "gray".to_string());
    m.insert("borderMuted".to_string(), "dark_gray".to_string());
    m.insert("success".to_string(), "green".to_string());
    m.insert("error".to_string(), "red".to_string());
    m.insert("warning".to_string(), "yellow".to_string());
    m.insert("info".to_string(), "accent_blue".to_string());
    m.insert("muted".to_string(), "gray".to_string());
    m.insert("dim".to_string(), "dim_gray".to_string());
    m.insert("text".to_string(), "text_color".to_string());
    m.insert("thinkingText".to_string(), "gray".to_string());
    m.insert("selectedBg".to_string(), "selected_bg".to_string());
    m.insert("userMessageBg".to_string(), "user_msg_bg".to_string());
    m.insert("userMessageText".to_string(), "text_color".to_string());
    m.insert("customMessageBg".to_string(), "custom_msg_bg".to_string());
    m.insert("customMessageText".to_string(), "text_color".to_string());
    m.insert("customMessageLabel".to_string(), "purple".to_string());
    m.insert("toolPendingBg".to_string(), "tool_pending_bg".to_string());
    m.insert("toolSuccessBg".to_string(), "tool_success_bg".to_string());
    m.insert("toolErrorBg".to_string(), "tool_error_bg".to_string());
    m.insert("toolTitle".to_string(), "text_color".to_string());
    m.insert("toolOutput".to_string(), "gray".to_string());
    m.insert("mdHeading".to_string(), "gold".to_string());
    m.insert("mdLink".to_string(), "sky".to_string());
    m.insert("mdLinkUrl".to_string(), "dim_gray".to_string());
    m.insert("mdCode".to_string(), "accent_blue".to_string());
    m.insert("mdCodeBlock".to_string(), "green".to_string());
    m.insert("mdCodeBlockBorder".to_string(), "gray".to_string());
    m.insert("mdQuote".to_string(), "gray".to_string());
    m.insert("mdQuoteBorder".to_string(), "gray".to_string());
    m.insert("mdHr".to_string(), "gray".to_string());
    m.insert("mdListBullet".to_string(), "accent_blue".to_string());
    m.insert("toolDiffAdded".to_string(), "green".to_string());
    m.insert("toolDiffRemoved".to_string(), "red".to_string());
    m.insert("toolDiffContext".to_string(), "gray".to_string());
    m.insert("syntaxComment".to_string(), "syntax_comment".to_string());
    m.insert("syntaxKeyword".to_string(), "syntax_keyword".to_string());
    m.insert("syntaxFunction".to_string(), "syntax_function".to_string());
    m.insert("syntaxVariable".to_string(), "syntax_variable".to_string());
    m.insert("syntaxString".to_string(), "syntax_string".to_string());
    m.insert("syntaxNumber".to_string(), "syntax_number".to_string());
    m.insert("syntaxType".to_string(), "syntax_type".to_string());
    m.insert("syntaxOperator".to_string(), "text_color".to_string());
    m.insert("syntaxPunctuation".to_string(), "text_color".to_string());
    m.insert("thinkingOff".to_string(), "dark_gray".to_string());
    m.insert(
        "thinkingMinimal".to_string(),
        "thinking_minimal".to_string(),
    );
    m.insert("thinkingLow".to_string(), "thinking_low".to_string());
    m.insert("thinkingMedium".to_string(), "thinking_medium".to_string());
    m.insert("thinkingHigh".to_string(), "thinking_high".to_string());
    m.insert("thinkingXhigh".to_string(), "thinking_xhigh".to_string());
    m.insert("bashMode".to_string(), "green".to_string());
    m
}

pub(super) fn color_value_to_color(value: &str, vars: &HashMap<String, Color>) -> Color {
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
