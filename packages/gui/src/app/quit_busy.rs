//! Quit busy predicate (no GPUI) — shared by close / Cmd+Q / menu Quit.

use piko_client_core::ClientState;
use piko_protocol::TurnStatus;

/// True when quitting would interrupt in-flight work that needs confirmation.
pub fn is_quit_busy(state: &ClientState) -> bool {
    let Some(live) = state.live_session.as_ref() else {
        return false;
    };
    if !live.pending_approvals.is_empty() {
        return true;
    }
    live.active_turns.iter().any(|t| {
        matches!(
            t.status,
            TurnStatus::Queued
                | TurnStatus::Running
                | TurnStatus::WaitingForApproval
                | TurnStatus::Cancelling
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_client_core::state::{ActiveTurn, LiveSession, PendingApproval};

    fn live_with(turns: Vec<ActiveTurn>, approvals: Vec<PendingApproval>) -> ClientState {
        let mut state = ClientState::default();
        state.live_session = Some(LiveSession {
            session_id: "s1".into(),
            cwd: "/tmp".into(),
            active_turns: turns,
            pending_approvals: approvals,
            ..Default::default()
        });
        state
    }

    fn turn(status: TurnStatus) -> ActiveTurn {
        ActiveTurn {
            turn_id: "t1".into(),
            agent_instance_id: "a1".into(),
            status,
        }
    }

    fn approval() -> PendingApproval {
        PendingApproval {
            approval_id: "ap1".into(),
            agent_instance_id: "a1".into(),
            tool_name: "bash".into(),
            tool_args: serde_json::json!({}),
            response_in_flight: false,
        }
    }

    #[test]
    fn idle_without_session_is_not_busy() {
        assert!(!is_quit_busy(&ClientState::default()));
    }

    #[test]
    fn idle_live_session_is_not_busy() {
        assert!(!is_quit_busy(&live_with(vec![], vec![])));
    }

    #[test]
    fn running_turn_is_busy() {
        assert!(is_quit_busy(&live_with(
            vec![turn(TurnStatus::Running)],
            vec![]
        )));
    }

    #[test]
    fn queued_turn_is_busy() {
        assert!(is_quit_busy(&live_with(
            vec![turn(TurnStatus::Queued)],
            vec![]
        )));
    }

    #[test]
    fn waiting_for_approval_turn_is_busy() {
        assert!(is_quit_busy(&live_with(
            vec![turn(TurnStatus::WaitingForApproval)],
            vec![]
        )));
    }

    #[test]
    fn cancelling_turn_is_busy() {
        assert!(is_quit_busy(&live_with(
            vec![turn(TurnStatus::Cancelling)],
            vec![]
        )));
    }

    #[test]
    fn completed_turn_is_not_busy() {
        assert!(!is_quit_busy(&live_with(
            vec![turn(TurnStatus::Completed)],
            vec![]
        )));
    }

    #[test]
    fn pending_approval_alone_is_busy() {
        assert!(is_quit_busy(&live_with(vec![], vec![approval()])));
    }
}
