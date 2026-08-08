//! TOML var/color resolution and complete-map construction.

use std::collections::HashMap;

use ratatui::style::Color;

use super::slots::{Theme, canonicalize_slot};
use super::{ColorValue, ThemeError};

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

    for (key, value) in colors {
        let color = resolve_color_value(value, vars)?;
        // Store under canonical slot name so construction is uniform.
        let canon = canonicalize_slot(key).to_string();
        resolved.insert(canon, color);
    }
    Ok(resolved)
}

fn resolve_color_value(
    value: &ColorValue,
    vars: &HashMap<String, Color>,
) -> Result<Color, ThemeError> {
    match value {
        ColorValue::Index(n) => Ok(Color::Indexed(*n)),
        ColorValue::Text(s) if s.is_empty() => Ok(Color::Reset),
        ColorValue::Text(s) if s.starts_with('#') => parse_hex(s),
        ColorValue::Text(s) => vars
            .get(s)
            .copied()
            .ok_or_else(|| ThemeError::MissingVar(s.clone())),
    }
}

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

/// Build a theme that must define every slot (built-in dark).
pub(super) fn theme_from_complete_map(
    name: String,
    colors: &HashMap<String, Color>,
) -> Result<Theme, ThemeError> {
    if Theme::SLOT_NAMES.len() != Theme::SLOT_COUNT {
        return Err(ThemeError::Parse(format!(
            "theme catalog size mismatch: {} names vs SLOT_COUNT {}",
            Theme::SLOT_NAMES.len(),
            Theme::SLOT_COUNT
        )));
    }

    for slot in Theme::SLOT_NAMES {
        if !colors.contains_key(*slot) {
            return Err(ThemeError::Parse(format!(
                "built-in theme '{name}' is missing required slot '{slot}'"
            )));
        }
    }

    let mut theme = empty_theme(name.clone());
    for (key, color) in colors {
        theme.apply_slot(key, *color);
    }
    theme.name = name;

    // Touch each slot through the string API so catalog get()/names stay live.
    for slot in Theme::SLOT_NAMES {
        let _ = theme.get(slot);
    }

    Ok(theme)
}

/// Placeholder theme; only valid after all slots are assigned.
fn empty_theme(name: String) -> Theme {
    let z = Color::Reset;
    Theme {
        name,
        bg_base: z,
        bg_elevated: z,
        bg_sunken: z,
        bg_highlight: z,
        bg_hover: z,
        bg_selected: z,
        bg_terminal: z,
        bg_visual: z,
        accent_user: z,
        accent_assistant: z,
        accent_thinking: z,
        accent_tool: z,
        accent_system: z,
        accent_error: z,
        accent_success: z,
        accent_running: z,
        accent_skill: z,
        accent: z,
        accent_alt: z,
        accent_plan: z,
        accent_model: z,
        accent_remember: z,
        text: z,
        text_secondary: z,
        dim: z,
        muted: z,
        gray_bright: z,
        success: z,
        error: z,
        warning: z,
        info: z,
        border: z,
        border_muted: z,
        prompt_border: z,
        prompt_border_active: z,
        selection_border: z,
        hover_border: z,
        command: z,
        path: z,
        running: z,
        scrollbar_bg: z,
        scrollbar_fg: z,
        diff_delete_bg: z,
        diff_delete_fg: z,
        diff_insert_bg: z,
        diff_insert_fg: z,
        diff_equal_fg: z,
        diff_gutter_fg: z,
        user_message_bg: z,
        user_message_text: z,
        tool_pending_bg: z,
        tool_success_bg: z,
        tool_error_bg: z,
        tool_title: z,
        tool_output: z,
        custom_message_bg: z,
        custom_message_text: z,
        custom_message_label: z,
        thinking_text: z,
        md_heading_h1: z,
        md_heading_h2: z,
        md_heading_h3: z,
        md_heading_h4: z,
        md_heading_h5: z,
        md_heading_h6: z,
        md_code: z,
        md_code_bg: z,
        md_text: z,
        md_muted: z,
        md_link: z,
        md_link_url: z,
        md_quote: z,
        md_quote_border: z,
        md_hr: z,
        md_list_bullet: z,
        md_task_checked: z,
        md_task_unchecked: z,
        syntax_comment: z,
        syntax_keyword: z,
        syntax_function: z,
        syntax_variable: z,
        syntax_string: z,
        syntax_number: z,
        syntax_type: z,
        syntax_operator: z,
        syntax_punctuation: z,
        thinking_off: z,
        thinking_minimal: z,
        thinking_low: z,
        thinking_medium: z,
        thinking_high: z,
        thinking_xhigh: z,
        bash_mode: z,
        paste_bg: z,
        paste_fg: z,
        paste_dim: z,
    }
}
