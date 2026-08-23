//! Timeline reading column rhythm and user-bubble expand prefs
//! (F-44 / F-46). Row kinds live on the timeline mapping.

use gpui::{Pixels, px};

use island::theme::metrics;

use super::timeline::{RowKind, TimelineRow, row_kind};

/// Gap above `cur`. First row is 0 so it does not stack on timeline `py`.
/// Consecutive pieces of one turn pack with zero gap into a single bubble.
pub fn row_gap_before(prev: Option<&TimelineRow>, cur: &TimelineRow) -> Pixels {
    let m = metrics();
    let Some(prev) = prev else {
        return px(0.);
    };
    if prev.turn_id().is_some() && prev.turn_id() == cur.turn_id() {
        return px(0.);
    }
    match (row_kind(prev), row_kind(cur)) {
        (RowKind::Assistant, RowKind::Assistant) => m.space_sm,
        (RowKind::System, _) | (_, RowKind::System) => m.space_md,
        _ => m.space_lg,
    }
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

    fn user(text: &str) -> TimelineRow {
        TimelineRow::User {
            id: format!("u-{text}"),
            text: text.to_string(),
        }
    }

    fn turn(id: &str, leads: bool) -> TimelineRow {
        TimelineRow::Assistant {
            id: format!("{id}#0"),
            turn_id: id.to_string(),
            leads_turn: leads,
            ends_turn: true,
            segments: Vec::new(),
        }
    }

    #[test]
    fn first_row_has_no_leading_gap() {
        assert_eq!(row_gap_before(None, &user("hi")), px(0.));
    }

    #[test]
    fn same_turn_pieces_pack_with_zero_gap() {
        let a = turn("t1", true);
        let b = TimelineRow::Assistant {
            id: "t1#1".into(),
            turn_id: "t1".into(),
            leads_turn: false,
            ends_turn: true,
            segments: Vec::new(),
        };
        assert_eq!(row_gap_before(Some(&a), &b), px(0.));
    }

    #[test]
    fn distinct_turns_keep_their_rhythm() {
        assert_eq!(
            row_gap_before(Some(&turn("t1", false)), &turn("t2", true)),
            metrics().space_sm
        );
    }

    #[test]
    fn system_uses_caption_gap() {
        let system = TimelineRow::System {
            id: "s".into(),
            label: "context".into(),
        };
        assert_eq!(
            row_gap_before(Some(&system), &user("hi")),
            metrics().space_md
        );
        assert_eq!(
            row_gap_before(Some(&user("hi")), &system),
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
