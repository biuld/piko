use super::*;
use crate::app::command::SurfaceAction;
use piko_protocol::{
    Command, CommandResult, HistoryAvailability, HistoryItemContent, HistoryItemDetail,
    HistoryItemRef, HistoryProvenance, MessageContent, SessionHistoryOverview,
};

fn inspected() -> AppState {
    let mut app = live_app();
    app.open_history(Some("archived".into()));
    app.history.pending_command_id = None;
    app.history.set_overview(SessionHistoryOverview {
        session_id: "archived".into(),
        cwd: "/project".into(),
        name: None,
        revision: 7,
        agents: Vec::new(),
        works: Vec::new(),
        next_cursor: None,
    });
    app
}

fn request_detail(app: &mut AppState) -> String {
    app.history_request(Command::SessionHistoryItemGet {
        command_id: "detail".into(),
        session_id: "archived".into(),
        item_ref: HistoryItemRef {
            revision: 7,
            token: "opaque".into(),
        },
    });
    app.history.pending_command_id.clone().unwrap()
}

fn detail_response(command_id: String) -> Event {
    Event::CommandResponse {
        command_id,
        result: Ok(CommandResult::SessionHistoryItemGot {
            detail: HistoryItemDetail {
                item_ref: HistoryItemRef {
                    revision: 7,
                    token: "opaque".into(),
                },
                provenance: HistoryProvenance::Fact,
                availability: HistoryAvailability::Available,
                content: Some(HistoryItemContent::Message {
                    message_id: "m".into(),
                    message: Message::User {
                        content: MessageContent::String("recorded body".into()),
                        timestamp: None,
                    },
                }),
            },
            timestamp: 0,
        }),
    }
}

#[test]
fn detail_error_is_local_and_explicit_open_can_retry() {
    let mut app = inspected();
    let command_id = request_detail(&mut app);
    assert!(app.history.detail_loading);
    assert!(!app.history.loading);
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Err("detail transport failed".into()),
    });
    assert_eq!(
        app.history.detail_error.as_deref(),
        Some("detail transport failed")
    );
    assert!(app.history.error.is_none());
    assert!(app.history.overview.is_some());
    let command_id = request_detail(&mut app);
    app.apply_event(detail_response(command_id));
    assert!(app.history.detail.is_some());
    assert!(app.history.detail_error.is_none());
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn leaving_or_filtering_detail_ignores_its_late_response() {
    for filter in [false, true] {
        let mut app = inspected();
        let command_id = request_detail(&mut app);
        if filter {
            app.dispatch(SurfaceAction::HistoryFilter.into());
        } else {
            assert!(!app.history.back());
        }
        app.apply_event(detail_response(command_id));
        assert!(app.history.detail.is_none());
        assert!(!app.history.detail_loading);
    }
}
