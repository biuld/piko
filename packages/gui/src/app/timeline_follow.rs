//! Timeline stick-to-bottom helpers using GPUI `ScrollHandle` measurements.

use gpui::{Pixels, ScrollHandle, px};

/// Distance from the scroll bottom within which we treat the view as "at bottom".
pub const NEAR_BOTTOM_THRESHOLD: Pixels = px(64.);

/// True when there is little/no overflow, or the viewport is within
/// [`NEAR_BOTTOM_THRESHOLD`] of the maximum scroll offset.
pub fn is_near_bottom(handle: &ScrollHandle) -> bool {
    let max = handle.max_offset();
    let offset = handle.offset();
    if max.height <= NEAR_BOTTOM_THRESHOLD {
        return true;
    }
    // offset.y is negative while scrolled down; at bottom ≈ -max.height.
    let distance_from_bottom = max.height + offset.y;
    distance_from_bottom <= NEAR_BOTTOM_THRESHOLD
}

/// Whether content growth should force a scroll-to-bottom.
///
/// Detached readers (`follow == false`) are never moved by streaming/growth.
/// Only explicit jump/submit reattaches and uses `pending_bottom`.
pub fn should_scroll_on_growth(follow: bool, content_grew: bool, pending_bottom: bool) -> bool {
    pending_bottom || (follow && content_grew)
}

/// Fingerprint of visible timeline content used to detect growth while following.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineContentFp {
    pub agent_id: Option<String>,
    pub row_count: usize,
    pub body_chars: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_ignores_content_growth() {
        assert!(!should_scroll_on_growth(false, true, false));
    }

    #[test]
    fn follow_scrolls_on_growth() {
        assert!(should_scroll_on_growth(true, true, false));
    }

    #[test]
    fn pending_bottom_always_scrolls() {
        assert!(should_scroll_on_growth(false, false, true));
        assert!(should_scroll_on_growth(true, false, true));
    }

    #[test]
    fn no_growth_no_pending_does_not_scroll() {
        assert!(!should_scroll_on_growth(true, false, false));
    }
}
