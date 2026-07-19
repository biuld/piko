use super::state::{IslandBody, IslandMedia, IslandPlaceholder};
use crate::theme::PikoIcon;

#[test]
fn placeholder_builder_sets_fields() {
    let p = IslandPlaceholder::new("No sessions")
        .piko_icon(PikoIcon::Circle)
        .subtitle("Create one to begin");
    assert_eq!(p.title.as_ref(), "No sessions");
    assert_eq!(
        p.subtitle.as_ref().map(|s| s.as_ref()),
        Some("Create one to begin")
    );
    assert!(matches!(p.media, Some(IslandMedia::Element(_))));
}

#[test]
fn body_loading_and_empty_skip_scroll_viewport() {
    assert!(!IslandBody::loading(IslandPlaceholder::new("Loading…")).uses_scroll_viewport());
    assert!(!IslandBody::empty(IslandPlaceholder::new("Empty")).uses_scroll_viewport());
    assert!(IslandBody::ready(gpui::div()).uses_scroll_viewport());
    assert!(!IslandBody::empty(IslandPlaceholder::new("x")).uses_scroll_viewport());
    assert!(!IslandBody::loading(IslandPlaceholder::new("x")).uses_scroll_viewport());
}
