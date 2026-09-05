use super::*;
use crate::app::command::SurfaceAction;

#[test]
fn history_slash_inspects_an_explicit_session_without_opening_it() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);

    let effects = app.try_slash_command("/history archived-1").unwrap();

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
fn history_response_keeps_the_live_session_and_populates_the_surface() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    let effects = app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: piko_protocol::SessionHistoryOverview {
                session_id: "archived-1".into(),
                cwd: "/old/project".into(),
                name: Some("old work".into()),
                revision: 7,
                agents: Vec::new(),
                works: Vec::new(),
                next_cursor: None,
            },
            timestamp: 1,
        }),
    });

    assert!(effects.is_empty());
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
    assert_eq!(app.history.session_id.as_deref(), Some("archived-1"));
    assert_eq!(app.history.overview.as_ref().unwrap().revision, 7);
}

fn overview_response(command_id: String, session_id: &str) -> Event {
    Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: piko_protocol::SessionHistoryOverview {
                session_id: session_id.into(),
                cwd: "/project".into(),
                name: None,
                revision: 7,
                agents: Vec::new(),
                works: Vec::new(),
                next_cursor: None,
            },
            timestamp: 1,
        }),
    }
}

#[test]
fn closed_history_ignores_late_response() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
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
    let previous = app.history.pending_command_id.clone().unwrap();
    app.open_history(Some("archived-2".into()));
    let current = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(previous, "archived-1"));
    assert_eq!(
        app.history.pending_command_id.as_deref(),
        Some(current.as_str())
    );
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
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.cycle_history_lens(SurfaceAction::HistoryLensPrevious);
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(Event::CommandResponse {
        command_id,
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
    let command_id = app.history.pending_command_id.clone().unwrap();
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
    let command_id = app.history.pending_command_id.clone().unwrap();
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

fn sample_overview(session_id: &str) -> piko_protocol::SessionHistoryOverview {
    piko_protocol::SessionHistoryOverview {
        session_id: session_id.into(),
        cwd: "/project".into(),
        name: None,
        revision: 7,
        agents: vec![
            piko_protocol::HistoryAgentSummary {
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                work_count: 1,
                origin: None,
                origin_availability: piko_protocol::HistoryAvailability::Available,
            },
            piko_protocol::HistoryAgentSummary {
                agent_instance_id: "child".into(),
                agent_spec_id: "worker".into(),
                parent_agent_instance_id: Some("root".into()),
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                work_count: 1,
                origin: None,
                origin_availability: piko_protocol::HistoryAvailability::Unavailable {
                    reason: "exact origin was not recorded".into(),
                },
            },
        ],
        works: vec![piko_protocol::HistoryWorkSummary {
            root_input_id: "input-child".into(),
            agent_instance_id: "child".into(),
            origin: piko_protocol::AgentInputOrigin::User,
            input_preview: "child work".into(),
            started_at: None,
            finished_at: None,
            outcome: None,
            step_count: 1,
            tool_count: 0,
            message_count: 1,
            usage: None,
        }],
        next_cursor: None,
    }
}

#[test]
fn agents_lens_nests_and_drills_into_agent_work() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview("archived-1"),
            timestamp: 1,
        }),
    });
    app.history.select_lens(1);
    assert_eq!(
        app.history.lens,
        crate::features::history::HistoryLens::Agents
    );
    let rows = app.history.visible_rows();
    assert!(matches!(
        &rows[0],
        crate::features::history::HistoryRow::Agent { agent, depth: 0 }
            if agent.agent_instance_id == "root"
    ));
    assert!(matches!(
        &rows[1],
        crate::features::history::HistoryRow::Agent { agent, depth: 1 }
            if agent.agent_instance_id == "child"
    ));
    app.history.selected = 1;
    app.history
        .drill_into_agent(app.history.selected_agent_id().unwrap());
    assert_eq!(app.history.agent_id.as_deref(), Some("child"));
    assert!(matches!(
        app.history.visible_rows()[0],
        crate::features::history::HistoryRow::Work(ref work) if work.root_input_id == "input-child"
    ));
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn transcript_lens_requests_an_independent_page() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    let effects = app.select_history_lens(2);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(
            piko_protocol::Command::SessionHistoryTranscriptPageGet { .. }
        )]
    ));
}

#[test]
fn facts_only_hides_diagnostic_children() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history.set_work(piko_protocol::HistoryWorkPage {
        session_id: "archived-1".into(),
        revision: 7,
        root_input_id: "input-1".into(),
        items: vec![piko_protocol::HistoryItemSummary {
            item_ref: piko_protocol::HistoryItemRef {
                revision: 7,
                token: "event:2:0".into(),
            },
            revision: 2,
            event_index: 0,
            committed_at: 2,
            kind: piko_protocol::HistoryItemKind::new("input"),
            provenance: piko_protocol::HistoryProvenance::Fact,
            availability: piko_protocol::HistoryAvailability::Available,
            relation: piko_protocol::HistoryRelation::default(),
            summary: "input admitted".into(),
            has_detail: true,
            children: vec![piko_protocol::HistoryItemSummary {
                item_ref: piko_protocol::HistoryItemRef {
                    revision: 7,
                    token: "event:3:0".into(),
                },
                revision: 3,
                event_index: 0,
                committed_at: 3,
                kind: piko_protocol::HistoryItemKind::new("prompt_assembly"),
                provenance: piko_protocol::HistoryProvenance::Diagnostic,
                availability: piko_protocol::HistoryAvailability::Available,
                relation: piko_protocol::HistoryRelation::default(),
                summary: "assembly".into(),
                has_detail: true,
                children: Vec::new(),
            }],
        }],
        next_cursor: None,
    });
    assert_eq!(app.history.visible_rows().len(), 2);
    app.history.provenance = piko_protocol::HistoryProvenanceFilter::Facts;
    assert_eq!(app.history.visible_rows().len(), 1);
}

#[test]
fn wide_layout_keeps_the_same_selection_as_narrow() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview("archived-1"),
            timestamp: 1,
        }),
    });
    app.history.selected = 0;
    app.history.last_width.set(40);
    let narrow = app.history.selected_work_id();
    app.history.last_width.set(120);
    let wide = app.history.selected_work_id();
    assert_eq!(narrow, wide);
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn empty_history_has_no_rows_and_keeps_the_active_session() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    assert_eq!(app.history.row_count(), 0);
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}

#[test]
fn agents_lens_shows_unavailable_legacy_origin() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview("archived-1"),
            timestamp: 1,
        }),
    });
    app.history.select_lens(1);
    let crate::features::history::HistoryRow::Agent { agent, depth: 1 } =
        &app.history.visible_rows()[1]
    else {
        panic!("expected nested child agent");
    };
    assert!(matches!(
        agent.origin_availability,
        piko_protocol::HistoryAvailability::Unavailable { .. }
    ));
}

#[test]
fn pointer_lens_hit_requests_journal() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    let actions = crate::ui::interaction::PointerComponent::pointer_event(
        &mut app.history,
        piko_tui_layout::ComponentHit {
            element: Some(crate::app::HitId::Mode(3)),
            rect: ratatui::layout::Rect::new(0, 0, 8, 1),
            x: 1,
            y: 0,
        },
        crate::ui::interaction::PointerGesture::Activate,
    );
    assert!(matches!(
        actions.as_slice(),
        [crate::app::command::Action::Surface(
            SurfaceAction::HistorySelectLens(3)
        )]
    ));
    let effects = app.select_history_lens(3);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(
            piko_protocol::Command::SessionHistoryJournalPageGet { .. }
        )]
    ));
}

#[test]
fn filter_hides_non_matching_work_rows() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview: sample_overview("archived-1"),
            timestamp: 1,
        }),
    });
    app.history.filter_editing = true;
    app.history.filter = "missing".into();
    assert_eq!(app.history.row_count(), 0);
    app.history.filter = "child".into();
    assert_eq!(app.history.row_count(), 1);
}

#[test]
fn work_list_fetches_the_next_page_near_the_end() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    let mut overview = sample_overview("archived-1");
    overview.next_cursor = Some("work:7:1".into());
    app.apply_event(Event::CommandResponse {
        command_id,
        result: Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot {
            overview,
            timestamp: 1,
        }),
    });
    app.history.selected = 0;
    let effects = app.history_next_page();
    assert!(matches!(
        effects.as_slice(),
        [Effect::Send(piko_protocol::Command::SessionHistoryOverviewGet {
            after_cursor: Some(cursor),
            ..
        })] if cursor == "work:7:1"
    ));
}

#[test]
fn back_leaves_work_detail_without_closing_or_changing_session() {
    let mut app = live_app();
    app.open_history(Some("archived-1".into()));
    let command_id = app.history.pending_command_id.clone().unwrap();
    app.apply_event(overview_response(command_id, "archived-1"));
    app.history.set_work(piko_protocol::HistoryWorkPage {
        session_id: "archived-1".into(),
        revision: 7,
        root_input_id: "input-1".into(),
        items: Vec::new(),
        next_cursor: None,
    });
    assert!(!app.history.back());
    assert!(app.history.work.is_none());
    assert_eq!(app.mode(), AppMode::Surface(SurfaceId::History));
    assert_eq!(app.session.id.as_deref(), Some("session-1"));
}
