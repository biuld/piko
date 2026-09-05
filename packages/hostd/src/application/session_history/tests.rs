#[test]
fn cursors_bind_snapshot_and_query_scope() {
    use super::{cursor_offset, page};
    use piko_protocol::ProtocolError;

    let (items, cursor) = page(vec![1, 2, 3], 0, 1, "item:root-1", 7);
    assert_eq!(items, vec![1]);
    assert_eq!(
        cursor_offset(cursor.as_deref(), "item:root-1", 7).unwrap(),
        1
    );
    assert!(cursor_offset(cursor.as_deref(), "item:root-2", 7).is_err());
    assert!(matches!(
        cursor_offset(cursor.as_deref(), "item:root-1", 8),
        Err(ProtocolError::HistoryRevisionChanged {
            current_revision: 8
        })
    ));
    assert!(cursor_offset(Some("work:7:bad"), "work", 7).is_err());
    assert!(cursor_offset(Some(&"x".repeat(1025)), "work", 7).is_err());
}
