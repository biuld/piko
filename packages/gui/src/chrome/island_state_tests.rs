//! Lightweight tests for island content-state helpers (no GPUI widgets).

use crate::chrome::island_state::{IslandBody, IslandMedia, IslandPlaceholder};

#[test]
fn placeholder_builder_sets_fields() {
    let p = IslandPlaceholder::new("No sessions")
        .icon("○")
        .subtitle("Create one to begin");
    assert_eq!(p.title.as_ref(), "No sessions");
    assert_eq!(
        p.subtitle.as_ref().map(|s| s.as_ref()),
        Some("Create one to begin")
    );
    assert!(matches!(p.media, Some(IslandMedia::Icon(_))));
    assert!(p.action.is_none());
}

#[test]
fn body_loading_and_empty_skip_scroll_viewport() {
    assert!(matches!(
        IslandBody::loading(IslandPlaceholder::new("Loading…")),
        IslandBody::Loading(_)
    ));
    assert!(matches!(
        IslandBody::empty(IslandPlaceholder::new("Empty")),
        IslandBody::Empty(_)
    ));
    assert!(!IslandBody::empty(IslandPlaceholder::new("x")).uses_scroll_viewport());
    assert!(!IslandBody::loading(IslandPlaceholder::new("x")).uses_scroll_viewport());
}
