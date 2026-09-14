#[test]
fn cursors_bind_snapshot_and_query_scope() {
    use super::cursor_offset;
    use piko_protocol::ProtocolError;

    let cursor = "agent:agent_a:7:5";
    assert_eq!(cursor_offset(Some(cursor), "agent:agent_a", 7).unwrap(), 5);
    assert!(cursor_offset(Some(cursor), "agent:agent_b", 7).is_err());
    assert!(matches!(
        cursor_offset(Some(cursor), "agent:agent_a", 8),
        Err(ProtocolError::HistoryRevisionChanged {
            current_revision: 8
        })
    ));
    assert!(cursor_offset(Some("agent:agent_a:7:bad"), "agent:agent_a", 7).is_err());
    assert!(cursor_offset(Some(&"x".repeat(1025)), "agent:agent_a", 7).is_err());
}
