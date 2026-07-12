use crate::domain::sessions::HostState;

pub(super) fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}
