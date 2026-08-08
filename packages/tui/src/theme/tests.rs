use super::resolve::*;

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
    let light = include_str!("../../resources/themes/light.toml");
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
