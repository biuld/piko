use super::*;
use piko_protocol::SessionSummary;

fn summary(session_id: &str) -> SessionSummary {
    SessionSummary {
        session_id: session_id.into(),
        cwd: "/tmp".into(),
        seq: 1,
        name: None,
        first_message: None,
        message_count: 0,
        created_at: None,
        modified_at: None,
        session_path: None,
        parent_session_path: None,
        integrity_error: None,
    }
}

#[test]
fn session_row_marks_active_session() {
    let row = session_row(&summary("s1"), Some("s1"), false, false);
    assert!(row.is_active);
    assert!(
        row.cells
            .iter()
            .all(|cell| !cell.text.eq_ignore_ascii_case("active")),
        "active state must come from the component default, not a hand-drawn cell"
    );
    let row = session_row(&summary("s1"), Some("s2"), false, false);
    assert!(!row.is_active);
}

#[test]
fn integrity_status_fits_its_column() {
    let mut broken = summary("broken");
    broken.integrity_error = Some("checksum mismatch".into());

    let row = session_row(&broken, None, false, false);
    let status = &row.cells[1].text;

    assert_eq!(status, "integrity error");
    assert!(status.chars().count() <= usize::from(SESSION_STATUS_COLUMN_WIDTH));
}

#[test]
fn all_rows_in_full_searchable_viewport_have_pointer_regions() {
    let mut sessions = SessionList::new();
    sessions.load((0..12).map(|i| summary(&format!("s{i:02}"))).collect());

    let regions = sessions.row_regions(Rect::new(0, 0, 80, 20));

    assert_eq!(regions.len(), 12);
    assert_eq!(regions.first().map(|(rect, _)| rect.y), Some(4));
    assert_eq!(regions.last().map(|(rect, _)| rect.y), Some(15));
}
