use super::*;

#[test]
fn section_rule_contains_label() {
    let theme = Theme::dark();
    let line = section_rule_line("Appearance", 40, &theme);
    let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(joined.starts_with("Appearance"));
    assert!(joined.contains('─'));
}

#[test]
fn pane_spec_builder() {
    let s = PaneSpec::new("Settings")
        .affix(PaneTitleAffix::Close)
        .search_filter("")
        .hints("Esc close")
        .focused(true);
    assert_eq!(s.title, "Settings");
    assert!(matches!(s.search, PaneSearch::Shown { filter: "", .. }));
    assert!(matches!(
        s.footer,
        PaneFooter::Hints(hints) if hints.single_line() == Some("Esc close")
    ));
    assert!(s.search_rule);
    assert_eq!(s.mode, PaneMode::Standard);
    assert_eq!(s.padding, PanePadding::UNIFORM_1);
    assert_eq!(s.title_affixes, vec![PaneTitleAffix::Close]);
}

#[test]
fn title_affixes_paint_semantics() {
    assert_eq!(PaneTitleAffix::Close.display(), "[x]");
    assert_eq!(PaneTitleAffix::label("tool: bash").display(), "tool: bash");
    assert_eq!(PaneTitleAffix::selection(2, 9).display(), "[2/9]");
    assert_eq!(
        PaneModeStrip::new(["Current", "All"], 1).display(),
        "Current | [All]"
    );
    assert_eq!(
        PaneModeStrip::new(["Default", "NoTools", "User", "Labeled", "All"], 1).display(),
        "Default | [NoTools] | User | Labeled | All"
    );
    assert_eq!(
        format_title_affixes(&[
            PaneTitleAffix::mode_strip(["Current", "All"], 0),
            PaneTitleAffix::selection(1, 3),
            PaneTitleAffix::Close,
        ]),
        "[Current] | All  [1/3]  [x]"
    );
}

#[test]
fn content_rect_matches_manual_geometry() {
    let area = Rect::new(0, 0, 60, 12);

    // Minimal: borders TOP|BOTTOM → (0,1,60,10); padding (1,0) →
    // (1,1,58,10); one-row hints footer → (1,1,58,9).
    let spec = PaneSpec::minimal("t").hints("help");
    assert_eq!(spec.content_rect(area), Some(Rect::new(1, 1, 58, 9)));

    // Standard: borders ALL → (1,1,58,10); padding (1,1) → (2,2,56,8);
    // one-row hints footer → (2,2,56,7).
    let spec = PaneSpec::new("t").hints("help");
    assert_eq!(spec.content_rect(area), Some(Rect::new(2, 2, 56, 7)));

    // Standard + search + rule + tip + reserved footer: content starts
    // below the first two zones and chrome consumes 5 rows in total.
    let spec = PaneSpec::new("t")
        .search_filter("x")
        .tip("tip")
        .footer(PaneFooter::Reserved { height: 2 });
    assert_eq!(spec.content_rect(area), Some(Rect::new(2, 4, 56, 3)));

    // Too small for any chrome: no content.
    assert_eq!(
        PaneSpec::new("t")
            .hints("h")
            .content_rect(Rect::new(0, 0, 60, 2)),
        None
    );
}

#[test]
fn mode_strip_clamps_active() {
    let strip = PaneModeStrip::new(["A", "B"], 99);
    assert_eq!(strip.display(), "A | [B]");
    assert_eq!(PaneModeStrip::new(Vec::<String>::new(), 0).display(), "");
}

#[test]
fn minimal_mode_defaults_sparse_chrome() {
    let s = PaneSpec::minimal("agents").search_filter("").hints("Esc");
    assert_eq!(s.mode, PaneMode::Minimal);
    assert_eq!(s.padding, PanePadding::new(1, 0));
    assert!(!s.search_rule);
    assert_eq!(s.borders, Borders::TOP.union(Borders::BOTTOM));
    // Explicit override wins after mode.
    let s = s.search_rule(true).padding(PanePadding::new(0, 0));
    assert!(s.search_rule);
    assert_eq!(s.padding, PanePadding::new(0, 0));
    let s = s.borders(Borders::ALL);
    assert_eq!(s.borders, Borders::ALL);
}

#[test]
fn no_search_hides_filter_zone() {
    let s = PaneSpec::minimal("slash suggestions")
        .search_filter("query")
        .no_search()
        .hints("Tab");
    assert!(matches!(s.search, PaneSearch::Hidden));
    assert!(!s.search_rule);
}

#[test]
fn footer_height_is_one_for_any_non_empty_hint() {
    assert_eq!(footer_height(PaneFooter::Hints("a\nb".into())), 1);
    assert_eq!(footer_height(PaneFooter::Hints("a".into())), 1);
    assert_eq!(footer_height(PaneFooter::Hints("".into())), 0);
}
