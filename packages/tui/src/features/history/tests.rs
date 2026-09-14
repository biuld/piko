use super::*;
use crate::{app::HitId, theme::Theme};
use piko_protocol::*;
use piko_tui_layout::Component;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn stream_item(index: usize) -> HistoryStreamItem {
    HistoryStreamItem {
        item_ref: HistoryItemRef {
            revision: 12,
            token: format!("opaque-{index}"),
        },
        revision: (index / 3 + 1) as u64,
        event_index: 0,
        committed_at: 0,
        kind: HistoryItemKind::new(if index.is_multiple_of(2) {
            "model_step"
        } else {
            "message"
        }),
        badge: if index.is_multiple_of(2) {
            "STEP 1".into()
        } else {
            "ASSISTANT".into()
        },
        relation: HistoryRelation {
            root_input_id: Some("input-inspect-history".into()),
            model_step_id: Some(format!("step-{index}")),
            ..Default::default()
        },
        summary: format!(
            "Step {} · inspect history rendering / 检查历史记录",
            index + 1
        ),
        status: None,
        duration_ms: None,
        has_detail: true,
    }
}

fn agent_summary() -> HistoryAgentSummary {
    HistoryAgentSummary {
        agent_instance_id: "agent-main".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
        lifecycle: AgentInstanceLifecycle::Open,
        work_count: 1,
    }
}

fn panel() -> HistoryPanel {
    let mut panel = HistoryPanel::default();
    panel.set_overview(SessionHistoryOverview {
        session_id: "session-history-fixture".into(),
        cwd: "/project/piko".into(),
        name: Some("History UI refinement".into()),
        revision: 12,
        agents: vec![agent_summary()],
        next_cursor: None,
    });
    panel.select_agent("agent-main".into());
    panel.set_stream(HistoryStreamPage {
        session_id: "session-history-fixture".into(),
        agent_instance_id: "agent-main".into(),
        revision: 12,
        items: (0..30).map(stream_item).collect(),
        next_cursor: Some("next".into()),
    });
    let mut lanes = HistoryLaneSummary {
        session_id: "session-history-fixture".into(),
        agent_instance_id: "agent-main".into(),
        revision: 12,
        blocks: Vec::new(),
        timing_available: true,
    };
    for index in 0..6 {
        lanes.blocks.push(HistoryLaneBlock {
            kind: HistoryLaneBlockKind::ModelStep,
            reference: HistoryItemRef {
                revision: 12,
                token: format!("opaque-{}", index * 2),
            },
            label: format!("step {index}"),
            status: "completed".into(),
            sequence: index,
            started_at: Some(i64::from(index) * 100),
            duration_ms: Some(50),
        });
    }
    panel.set_lanes(lanes);
    panel
}

fn open_detail(panel: &mut HistoryPanel) {
    panel.opened_row = panel.visible_rows().get(panel.selected).cloned();
    panel.set_detail(HistoryItemDetail {
        item_ref: stream_item(panel.selected).item_ref,
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        content: Some(HistoryItemContent::Message {
            message_id: "message-long-body".into(),
            message: Message::User {
                content: MessageContent::String(
                    (0..80)
                        .map(|i| format!("Evidence line {i:02}: keep complete recorded content."))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                timestamp: None,
            },
        }),
        diagnostic: None,
    });
}

fn render(panel: &HistoryPanel, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            panel.render(
                frame,
                frame.area(),
                &HistoryCtx {
                    theme: &Theme::dark(),
                    hints: Some("↑/↓ move · Enter open · ← back · r refresh"),
                },
            )
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn shown(buffer: &ratatui::buffer::Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

#[test]
fn detail_scroll_survives_paint_and_back_restores_list() {
    let mut panel = panel();
    panel.selected = 23;
    render(&panel, 120, 24);
    let list_top = panel.viewport.get().top();
    assert!(list_top > 0);
    open_detail(&mut panel);
    panel.active_pane = PaneSide::Second;
    panel.select_detail_tab(1); // Payload carries the long body.
    render(&panel, 120, 24);
    for _ in 0..30 {
        panel.select_next();
    }
    let top = panel.detail_viewport.get().top();
    let buffer = render(&panel, 120, 24);
    assert!(top > 0);
    assert_eq!(panel.detail_viewport.get().top(), top);
    assert_eq!(panel.viewport.get().top(), list_top);
    assert!(shown(&buffer).contains("Evidence line"));
    assert!(!panel.back());
    render(&panel, 120, 24);
    assert_eq!(panel.selected, 23);
    assert_eq!(panel.viewport.get().top(), list_top);
}

#[test]
fn prepared_hits_match_first_paint_scroll_and_resize() {
    let mut panel = panel();
    panel.selected = 23;
    for width in [120, 60, 100, 40] {
        let area = Rect::new(0, 0, width, 24);
        let hits =
            <HistoryPanel as Component<HitId, HistoryCtx<'_>>>::component_regions(&panel, area);
        let buffer = render(&panel, width, 24);
        assert_eq!(hits, *panel.painted_regions.borrow());
        if let Some((rect, _)) = hits.iter().find(|(_, id)| *id == HitId::Row(23)) {
            assert!(buffer[(rect.x, rect.y)].symbol().contains('›'));
        }
        assert_eq!(panel.selected, 23);
    }
    open_detail(&mut panel);
    render(&panel, 60, 24);
    assert!(panel.shows_detail_only());
    assert!(
        panel
            .painted_regions
            .borrow()
            .iter()
            .any(|(_, id)| *id == HitId::Content)
    );
    assert!(
        !panel
            .painted_regions
            .borrow()
            .iter()
            .any(|(_, id)| matches!(id, HitId::Row(_)))
    );
    render(&panel, 120, 24);
    assert!(panel.is_wide());
    assert!(panel.detail.is_some());
    assert_eq!(panel.selected, 23);
}

#[test]
fn visible_stream_rows_are_clickable_after_scrolling() {
    use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

    let mut panel = panel();
    panel.selected = 23;
    let area = Rect::new(0, 0, 120, 24);
    let hits = <HistoryPanel as Component<HitId, HistoryCtx<'_>>>::component_regions(&panel, area);
    let (rect, row_index) = hits
        .iter()
        .find_map(|(rect, id)| match id {
            HitId::Row(index) if *index != panel.selected => Some((*rect, *index)),
            _ => None,
        })
        .expect("wide stream list should expose visible row hits");

    let actions = panel.pointer_event(
        ComponentHit {
            element: Some(HitId::Row(row_index)),
            rect,
            x: rect.x,
            y: rect.y,
        },
        PointerGesture::Activate,
    );

    assert_eq!(panel.selected, row_index);
    assert!(matches!(
        actions.as_slice(),
        [crate::app::command::Action::Surface(
            crate::app::command::SurfaceAction::Confirm
        )]
    ));
}

#[test]
fn lane_strip_has_a_full_width_border_above_the_content() {
    let panel = panel();
    let area = Rect::new(0, 0, 120, 24);
    let divider = panel
        .prepare_layout(area)
        .and_then(|layout| layout.lane_divider)
        .expect("lane strip should reserve a divider");
    let buffer = render(&panel, area.width, area.height);

    for x in divider.x..divider.right() {
        assert_eq!(buffer[(x, divider.y)].symbol(), "─");
    }
}

#[test]
fn visual_fixtures() {
    let Some(directory) = std::env::var_os("PIKO_HISTORY_QA_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let stream_panel = panel();
    for width in [40, 60, 120] {
        export_frame(&stream_panel, &directory, &format!("stream-{width}"), width);
    }
    let mut detail_panel = panel();
    open_detail(&mut detail_panel);
    export_frame(&detail_panel, &directory, "detail-wide", 120);
    export_frame(&detail_panel, &directory, "detail-compact", 40);
    detail_panel.detail_viewport.get_mut().scroll_by(35);
    export_frame(&detail_panel, &directory, "detail-scrolled", 120);
    detail_panel.detail_error = Some("Detail unavailable: transport failed".into());
    export_frame(&detail_panel, &directory, "detail-error", 120);
    detail_panel.clear_detail();
    detail_panel.filter = "no match".into();
    export_frame(&detail_panel, &directory, "filtered-empty", 60);
}

fn export_frame(panel: &HistoryPanel, directory: &std::path::Path, name: &str, width: u16) {
    let buffer = render(panel, width, 28);
    let cells = buffer.content.iter().map(|cell| serde_json::json!({"text": cell.symbol(), "fg": format!("{:?}", cell.fg), "bg": format!("{:?}", cell.bg)})).collect::<Vec<_>>();
    std::fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec(&serde_json::json!({"width": width, "height": 28, "cells": cells}))
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn wheel_over_list_keeps_detail_position_and_wheel_over_detail_keeps_selection() {
    use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};
    let mut panel = panel();
    panel.selected = 10;
    open_detail(&mut panel);
    panel.select_detail_tab(1); // Payload carries the long body.
    render(&panel, 120, 24);
    let detail = panel.painted_split.get().unwrap().second.unwrap().content;
    panel.pointer_event(
        ComponentHit {
            element: Some(HitId::Content),
            rect: detail,
            x: detail.x,
            y: detail.y + 2,
        },
        PointerGesture::ScrollDown,
    );
    assert_eq!(panel.selected, 10);
    assert_eq!(panel.detail_viewport.get().top(), 3);
    let list = panel.painted_split.get().unwrap().first.unwrap().content;
    panel.pointer_event(
        ComponentHit {
            element: Some(HitId::Row(10)),
            rect: list,
            x: list.x,
            y: list.y + 2,
        },
        PointerGesture::ScrollDown,
    );
    assert_eq!(panel.selected, 10);
    assert_eq!(panel.viewport.get().top(), 3);
    assert_eq!(panel.detail_viewport.get().top(), 3);
    assert!(
        matches!(panel.opened_row, Some(HistoryRow::Stream(ref item)) if item.item_ref.token == "opaque-10")
    );
}

#[test]
fn summary_shows_journal_metadata_once_without_a_repeated_evidence_section() {
    let mut panel = panel();
    panel.selected = 2;
    open_detail(&mut panel);

    let text = shown(&render(&panel, 120, 24));

    assert!(text.contains("Journal"));
    assert!(text.contains("Position"));
    assert!(!text.contains("Journal evidence"));
}

#[test]
fn list_projection_clones_only_the_requested_visible_range() {
    let mut panel = panel();
    panel.filter = "inspect history".into();

    let rows = panel.visible_rows_range(8..11);

    assert_eq!(rows.len(), 3);
    for (offset, row) in rows.iter().enumerate() {
        let HistoryRow::Stream(item) = row else {
            panic!("expected stream row");
        };
        assert_eq!(item.item_ref.token, format!("opaque-{}", offset + 8));
    }
}

#[test]
fn filtered_count_retains_loaded_scope() {
    let mut panel = panel();
    panel.filter = "Step 12 ·".into();
    assert_eq!(panel.row_count(), 1);
    assert_eq!(panel.loaded_row_count(), 30);
    let buffer = render(&panel, 60, 24);
    assert!(shown(&buffer).contains("1 / 30 loaded"));
    assert!(!shown(&buffer).contains("more"));
}

#[test]
fn compact_detail_feedback_wraps_and_identifies_the_opened_item() {
    let mut panel = panel();
    panel.selected = 10;
    open_detail(&mut panel);
    panel.detail_error = Some("Transport failed while fetching the recorded body".into());
    let buffer = render(&panel, 40, 40);
    let text = shown(&buffer);
    let words: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '│')
        .collect();
    assert!(words.contains("reopentoretry"), "{words}");
    panel.detail_error = None;
    panel.detail_loading = true;
    let buffer = render(&panel, 40, 40);
    assert!(shown(&buffer).contains("Loading selected detail"));
}

#[test]
fn lane_blocks_align_across_rows_and_select_their_stream_row() {
    let mut panel = panel();
    let buffer = render(&panel, 120, 24);
    // Lane strip labels are painted.
    assert!(shown(&buffer).contains("Model"));
    assert!(shown(&buffer).contains("Tools"));
    // A lane block hit selects its stream row by persisted token.
    panel.select_lane_block(3);
    let rows = panel.visible_rows();
    let selected = rows.get(panel.selected).unwrap();
    let HistoryRow::Stream(item) = selected else {
        panic!("expected a stream row");
    };
    assert_eq!(item.item_ref.token, "opaque-6");
}

#[test]
fn lane_strip_fills_the_width_when_blocks_fit() {
    let panel = panel();
    <HistoryPanel as Component<HitId, HistoryCtx<'_>>>::component_regions(
        &panel,
        Rect::new(0, 0, 120, 24),
    );
    assert_eq!(panel.lane_unit.get(), 0, "fit mode does not scroll");
    assert_eq!(panel.lane_viewport.get().top(), 0);
}

#[test]
fn lane_strip_scrolls_horizontally_when_blocks_overflow() {
    use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

    let mut panel = panel();
    let blocks = (0..200u32)
        .map(|index| HistoryLaneBlock {
            kind: HistoryLaneBlockKind::ToolCall,
            reference: HistoryItemRef {
                revision: 12,
                token: format!("opaque-{}", index % 30),
            },
            label: "exec_command".into(),
            status: "completed".into(),
            sequence: index * 2,
            started_at: Some(0),
            duration_ms: Some(1),
        })
        .collect();
    panel.set_lanes(HistoryLaneSummary {
        session_id: "session-history-fixture".into(),
        agent_instance_id: "agent-main".into(),
        revision: 12,
        blocks,
        timing_available: true,
    });
    let area = Rect::new(0, 0, 80, 24);
    <HistoryPanel as Component<HitId, HistoryCtx<'_>>>::component_regions(&panel, area);
    assert_eq!(panel.lane_unit.get(), 2, "overflow engages scroll mode");
    let strip = panel.lane_strip_rect.get().unwrap();
    panel.pointer_event(
        ComponentHit {
            element: None,
            rect: strip,
            x: strip.x + 4,
            y: strip.y,
        },
        PointerGesture::ScrollDown,
    );
    assert!(panel.lane_viewport.get().top() > 0);
}
