use super::*;
use crate::app::command::SurfaceAction;

#[test]
fn trajectory_slash_inspects_an_explicit_session_without_opening_it() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);

    let effects = app.try_slash_command("/trajectory archived-1").unwrap();

    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert_eq!(app.mode(), AppMode::Surface(SurfaceId::History));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::SessionHistoryOverviewGet {
            session_id,
            ..
        })] if session_id == "archived-1"
    ));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(piko_protocol::Command::SessionOpen { .. })
    )));
}

#[test]
fn retired_history_slash_is_not_registered() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);

    assert!(app.try_slash_command("/history archived-1").is_none());
}

fn agent(id: &str, parent: Option<&str>) -> piko_protocol::HistoryAgentSummary {
    piko_protocol::HistoryAgentSummary {
        agent_instance_id: id.into(),
        agent_spec_id: id.into(),
        parent_agent_instance_id: parent.map(str::to_string),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        work_count: 1,
    }
}

fn overview_response(command_id: String, session_id: &str) -> Event {
    Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview(session_id),
            timestamp: 1,
        }),
    }
}

fn sample_overview(session_id: &str) -> piko_protocol::SessionHistoryOverview {
    piko_protocol::SessionHistoryOverview {
        session_id: session_id.into(),
        cwd: "/project".into(),
        name: None,
        revision: 7,
        agents: vec![agent("root", None), agent("child", Some("root"))],
        next_cursor: None,
    }
}

fn stream_page(agent_id: &str, tokens: &[&str]) -> piko_protocol::HistoryStreamPage {
    piko_protocol::HistoryStreamPage {
        session_id: "archived-1".into(),
        agent_instance_id: agent_id.into(),
        revision: 7,
        items: tokens
            .iter()
            .map(|token| piko_protocol::HistoryStreamItem {
                item_ref: piko_protocol::HistoryItemRef {
                    revision: 7,
                    token: (*token).into(),
                },
                revision: 2,
                event_index: 0,
                committed_at: 2,
                kind: piko_protocol::HistoryItemKind::new("input"),
                badge: "USER".into(),
                relation: piko_protocol::HistoryRelation::default(),
                summary: "input admitted".into(),
                status: None,
                duration_ms: None,
                has_detail: true,
            })
            .collect(),
        next_cursor: None,
    }
}

#[test]
fn overview_response_keeps_the_live_session_and_autoloads_the_root_stream() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    let effects = app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview("archived-1"),
            timestamp: 1,
        }),
    });

    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert_eq!(app.history.session_id.as_deref(), Some("archived-1"));
    assert_eq!(app.history.overview.as_ref().unwrap().revision, 7);
    // The default root agent stream and lane strip were requested.
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(piko_protocol::Command::SessionHistoryAgentStreamGet {
            agent_instance_id,
            ..
        }) if agent_instance_id == "root"
    )));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(piko_protocol::Command::SessionHistoryLaneGet { .. })
    )));
}

#[test]
fn closed_history_ignores_late_response() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.pop_focus();
    app.apply_event(overview_response(command_id, "archived-1"));
    assert_eq!(app.mode(), AppMode::Chat);
    assert!(app.history.overview.is_none());
    assert!(app.history.session_id.is_none());
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn switching_inspected_session_ignores_previous_request() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let previous = app.history.pending_commands[0].clone();
    app.open_history(Some("archived-2".into()));
    let current = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(previous, "archived-1"));
    assert_eq!(app.history.pending_commands[0], current);
    assert!(app.history.overview.is_none());
    app.apply_event(overview_response(current, "archived-2"));
    assert_eq!(
        app.history.overview.as_ref().unwrap().session_id,
        "archived-2"
    );
}

#[test]
fn history_failure_preserves_loaded_snapshot_and_stops_loading() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    let stream_command_id = app.history.pending_commands[0].clone();
    app.apply_event(Event::CommandResponse {
        command_id: stream_command_id,
        result: Err("transport failed".into()),
    });
    assert!(!app.history.loading);
    assert_eq!(app.history.error.as_deref(), Some("transport failed"));
    assert_eq!(app.history.overview.as_ref().unwrap().revision, 7);
}

#[test]
fn changed_history_revision_restarts_without_touching_active_session() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    let effects = app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::HistoryRevisionChanged {
            session_id: "archived-1".into(),
            current_revision: 8,
        }),
    });
    assert!(
        matches!(&effects[0], Effect::Send(piko_protocol::Command::SessionHistoryOverviewGet {
        after_cursor: None, session_id, ..
    }) if session_id == "archived-1")
    );
    assert!(app.history.overview.is_none());
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn history_without_active_session_uses_an_isolated_chooser() {
    let mut app = live_app();
    app.session.id = None;
    let effects = app.open_history(None);
    assert!(app.history.choosing_session);
    assert!(matches!(
        &effects[0],
        Effect::Send(piko_protocol::Command::SessionList { .. })
    ));
    let command_id = app.history.pending_commands[0].clone();
    app.pop_focus();
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionListed {
            sessions: Vec::new(),
            timestamp: 1,
        }),
    });
    assert_eq!(app.mode(), AppMode::Chat);
    assert!(app.session.id.is_none());
}

#[test]
fn agent_chooser_lists_nested_agents_and_switches_streams() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    assert_eq!(app.history.agent_id.as_deref(), Some("root"));

    let effects = app.select_history_agent(1);
    assert_eq!(app.history.agent_id.as_deref(), Some("child"));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(piko_protocol::Command::SessionHistoryAgentStreamGet {
            agent_instance_id,
            ..
        }) if agent_instance_id == "child"
    )));
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn stream_page_replaces_pages_per_agent_and_accumulates_cursors() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history.set_stream(stream_page("root", &["event:2:0"]));
    assert_eq!(app.history.row_count(), 1);
    let mut second = stream_page("root", &["event:3:0"]);
    second.next_cursor = Some("agent:root:7:2".into());
    app.history.set_stream(second);
    assert_eq!(app.history.row_count(), 2);
    assert!(app.history.has_more());

    // A different agent replaces the stream.
    app.history.set_stream(stream_page("child", &["event:4:0"]));
    assert_eq!(app.history.row_count(), 1);
    assert!(!app.history.has_more());
}

#[test]
fn pointer_agent_chip_and_lane_block_hits_drive_selection() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history
        .set_stream(stream_page("root", &["event:2:0", "event:3:0"]));
    let mut lanes = piko_protocol::HistoryLaneSummary {
        session_id: "archived-1".into(),
        agent_instance_id: "root".into(),
        revision: 7,
        blocks: Vec::new(),
        timing_available: false,
    };
    lanes.blocks.push(piko_protocol::HistoryLaneBlock {
        kind: piko_protocol::HistoryLaneBlockKind::ToolCall,
        reference: piko_protocol::HistoryItemRef {
            revision: 7,
            token: "event:3:0".into(),
        },
        label: "bash".into(),
        status: "committed".into(),
        sequence: 0,
        started_at: None,
        duration_ms: None,
    });
    app.history.set_lanes(lanes);

    // Agent chip hit requests that agent stream.
    let actions = crate::ui::interaction::PointerComponent::pointer_event(
        &mut app.history,
        crate::ui::interaction::ComponentHit {
            element: Some(crate::app::HitId::Mode(1)),
            rect: ratatui::layout::Rect::new(0, 0, 8, 1),
            x: 1,
            y: 0,
        },
        crate::ui::interaction::PointerGesture::Activate,
    );
    assert!(matches!(
        actions.as_slice(),
        [crate::app::command::Action::Surface(
            SurfaceAction::HistorySelectAgent(1)
        )]
    ));

    // Lane block hit selects the stream row with the same token.
    app.history.selected = 0;
    app.history.select_lane_block(0);
    assert_eq!(app.history.selected, 1);
}

#[test]
fn filter_hides_non_matching_stream_rows() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history
        .set_stream(stream_page("root", &["event:2:0", "event:3:0"]));
    app.history.filter_editing = true;
    app.history.filter = "missing".into();
    assert_eq!(app.history.row_count(), 0);
    app.history.filter = "admitted".into();
    assert_eq!(app.history.row_count(), 2);
}

#[test]
fn stream_list_fetches_the_next_page_near_the_end() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    let mut page = stream_page("root", &["event:2:0", "event:3:0"]);
    page.next_cursor = Some("agent:root:7:2".into());
    app.history.set_stream(page);
    app.history.selected = 0;
    let effects = app.history_next_page();
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::SessionHistoryAgentStreamGet {
            after_cursor: Some(cursor),
            ..
        })] if cursor == "agent:root:7:2"
    ));
}

#[test]
fn reopening_a_cached_history_row_does_not_request_detail_again() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history.set_stream(stream_page("root", &["event:2:0"]));
    app.history.pending_commands.clear();
    app.history.set_detail(piko_protocol::HistoryItemDetail {
        item_ref: piko_protocol::HistoryItemRef {
            revision: 7,
            token: "event:2:0".into(),
        },
        provenance: piko_protocol::HistoryProvenance::Fact,
        availability: piko_protocol::HistoryAvailability::Available,
        content: None,
        diagnostic: None,
    });
    app.history.clear_detail();

    let effects = app.open_history_detail();

    assert!(effects.is_empty());
    assert_eq!(
        app.history
            .detail
            .as_ref()
            .map(|detail| detail.item_ref.token.as_str()),
        Some("event:2:0")
    );
    assert!(!app.history.detail_loading);
}

#[test]
fn back_on_the_stream_closes_and_keeps_the_active_session() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history.set_stream(stream_page("root", &["event:2:0"]));
    assert!(app.history.back());
    assert!(app.history.detail.is_none());
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn empty_history_has_no_rows_and_keeps_the_active_session() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_commands[0].clone();
    app.apply_event(overview_response(command_id, "archived-1"));
    assert_eq!(app.history.row_count(), 0);
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}
