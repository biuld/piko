//! Timeline reading column rhythm and user-bubble expand prefs
//! (F-44 / F-46). Row kinds live on the timeline mapping.

use gpui::{Pixels, px};

use island::theme::metrics;

use super::timeline::RowKind;

/// Gap above `next`. First row is 0 so it does not stack on timeline `py`.
pub fn row_gap_before(prev: Option<RowKind>, next: RowKind) -> Pixels {
    let m = metrics();
    let Some(prev) = prev else {
        return px(0.);
    };
    if matches!((prev, next), (RowKind::Assistant, RowKind::Assistant)) {
        return m.space_sm;
    }
    if prev == RowKind::System || next == RowKind::System {
        return m.space_md;
    }
    m.space_lg
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockExpandPref {
    #[default]
    Untouched,
    Expanded,
    Collapsed,
}

pub fn user_pref_open(pref: BlockExpandPref) -> bool {
    pref == BlockExpandPref::Expanded
}

/// User bubbles are the only inline-toggling blocks left (F-46).
pub fn toggle_expand(_pref: BlockExpandPref, currently_open: bool) -> BlockExpandPref {
    if currently_open {
        BlockExpandPref::Collapsed
    } else {
        BlockExpandPref::Expanded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_row_has_no_leading_gap() {
        assert_eq!(row_gap_before(None, RowKind::User), px(0.));
    }

    #[test]
    fn turn_to_turn_is_tight() {
        assert_eq!(
            row_gap_before(Some(RowKind::Assistant), RowKind::Assistant),
            metrics().space_sm
        );
    }

    #[test]
    fn system_uses_caption_gap() {
        assert_eq!(
            row_gap_before(Some(RowKind::System), RowKind::User),
            metrics().space_md
        );
    }

    #[test]
    fn user_expand_pref_inverts_on_toggle() {
        assert!(user_pref_open(BlockExpandPref::Expanded));
        assert!(!user_pref_open(toggle_expand(
            BlockExpandPref::Untouched,
            true
        )));
    }
}
