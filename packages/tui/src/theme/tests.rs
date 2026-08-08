use super::*;
use ratatui::style::Color;

#[test]
fn test_dark_theme_loads() {
    let theme = Theme::dark();
    assert_eq!(theme.name, "dark");
    assert_eq!(Theme::SLOT_NAMES.len(), Theme::SLOT_COUNT);
    // Core tokens present
    assert_ne!(theme.accent, Color::Reset);
    assert_ne!(theme.text, Color::Reset);
    assert_ne!(theme.border, Color::Reset);
    assert_ne!(theme.border_muted, Color::Reset);
    // Brand accent is #5f87ff (selection); borders are neutral chrome only.
    assert_eq!(theme.accent, Color::Rgb(95, 135, 255));
    assert_ne!(theme.border, theme.accent);
}

#[test]
fn test_light_theme_loads() {
    let theme = Theme::light();
    assert_eq!(theme.name, "light");
    assert_ne!(theme.accent, Color::Reset);
    assert_eq!(theme.bg_base, Color::Rgb(0xee, 0xee, 0xee));
}

#[test]
fn test_all_slots_present_on_dark() {
    let theme = Theme::dark();
    for key in Theme::SLOT_NAMES {
        let color = theme.get(key);
        // Only empty-string tokens may be Reset; built-in dark paints every slot.
        assert_ne!(
            color,
            Color::Reset,
            "slot '{key}' should not be Reset on dark"
        );
    }
}

#[test]
fn test_legacy_aliases_resolve() {
    let theme = Theme::dark();
    assert_eq!(theme.get("userMessageBg"), theme.user_message_bg);
    assert_eq!(theme.get("selectedBg"), theme.bg_selected);
    assert_eq!(theme.get("mdHeading"), theme.md_heading_h1);
    assert_eq!(theme.get("toolDiffAdded"), theme.diff_insert_fg);
    assert_eq!(theme.get("borderMuted"), theme.border_muted);
    assert_eq!(theme.get("accentAlt"), theme.accent_alt);
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
    assert_eq!(theme.accent, Color::Rgb(255, 0, 0));
    assert_ne!(theme.border, Color::Reset);
    assert_ne!(theme.dim, Color::Reset);
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

#[test]
fn test_role_accents_distinct() {
    let theme = Theme::dark();
    assert_ne!(theme.accent_user, theme.accent_assistant);
    assert_ne!(theme.accent_tool, theme.accent_system);
}

#[test]
fn test_md_heading_levels() {
    let theme = Theme::dark();
    assert_eq!(theme.md_heading(1), theme.md_heading_h1);
    assert_eq!(theme.md_heading(6), theme.md_heading_h6);
    assert_eq!(theme.md_heading(0), theme.md_heading_h1);
    assert_eq!(theme.md_heading(99), theme.md_heading_h6);
}
