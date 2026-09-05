use super::*;
use crate::{app::HitId, theme::Theme};
use piko_protocol::*;
use piko_tui_layout::Component;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn item(index: usize) -> HistoryItemSummary {
    HistoryItemSummary {
        item_ref: HistoryItemRef {
            revision: 12,
            token: format!("opaque-{index}"),
        },
        revision: (index / 3 + 1) as u64,
        event_index: 0,
        committed_at: 0,
        kind: HistoryItemKind::new("model_step"),
        provenance: HistoryProvenance::Fact,
        availability: HistoryAvailability::Available,
        relation: HistoryRelation {
            root_input_id: Some("input-inspect-history".into()),
            model_step_id: Some(format!("step-{index}")),
            ..Default::default()
        },
        summary: format!(
            "Step {} · inspect history rendering / 检查历史记录",
            index + 1
        ),
        has_detail: true,
        children: Vec::new(),
    }
}

fn panel() -> HistoryPanel {
    let mut panel = HistoryPanel::default();
    panel.set_overview(SessionHistoryOverview {
        session_id: "session-history-fixture".into(),
        cwd: "/project/piko".into(),
        name: Some("History UI refinement".into()),
        revision: 12,
        agents: vec![HistoryAgentSummary {
            agent_instance_id: "agent-main".into(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: AgentInstanceLifecycle::Open,
            work_count: 1,
            origin: None,
            origin_availability: HistoryAvailability::Available,
        }],
        works: vec![HistoryWorkSummary {
            root_input_id: "input-inspect-history".into(),
            agent_instance_id: "agent-main".into(),
            origin: AgentInputOrigin::User,
            input_preview: "Inspect history rendering / 检查历史记录".into(),
            started_at: None,
            finished_at: None,
            outcome: Some(AgentWorkProcessingStatus::Succeeded),
            step_count: 12,
            tool_count: 6,
            message_count: 10,
            usage: None,
        }],
        next_cursor: None,
    });
    panel.set_work(HistoryWorkPage {
        session_id: "session-history-fixture".into(),
        revision: 12,
        root_input_id: "input-inspect-history".into(),
        items: (0..30).map(item).collect(),
        next_cursor: Some("next".into()),
    });
    panel
}

fn open_detail(panel: &mut HistoryPanel) {
    panel.opened_row = panel.visible_rows().get(panel.selected).cloned();
    panel.set_detail(HistoryItemDetail {
        item_ref: item(panel.selected).item_ref,
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
                    hints: Some("↑/↓ move · Enter open · ← back · Tab lens · r refresh"),
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
        let (rect, _) = hits.iter().find(|(_, id)| *id == HitId::Row(23)).unwrap();
        assert!(buffer[(rect.x, rect.y)].symbol().contains('›'));
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
fn unavailable_diagnostics_keep_both_labels_and_unknown_work_is_not_pending() {
    let theme = Theme::dark();
    let mut diagnostic = item(0);
    diagnostic.provenance = HistoryProvenance::Diagnostic;
    diagnostic.availability = HistoryAvailability::Unavailable {
        reason: "capture absent".into(),
    };
    for width in [32, 60, 120] {
        let line = present::row_line(
            width,
            false,
            &HistoryRow::Item {
                item: diagnostic.clone(),
                depth: 30,
            },
            &theme,
        );
        let text = line.to_string();
        assert!(text.contains("diag unavailable"));
        assert!(line.width() <= usize::from(width));
    }
}

#[test]
fn agents_work_drilldown_uses_items_and_restores_parent_selection() {
    let mut panel = panel();
    panel.back();
    panel.select_lens(1);
    panel.drill_into_agent("agent-main".into());
    panel.set_work(HistoryWorkPage {
        session_id: "session-history-fixture".into(),
        revision: 12,
        root_input_id: "input-inspect-history".into(),
        items: vec![item(1)],
        next_cursor: None,
    });
    assert!(matches!(panel.visible_rows()[0], HistoryRow::Item { .. }));
    assert!(!panel.back());
    assert!(matches!(panel.visible_rows()[0], HistoryRow::Work(_)));
    assert!(!panel.back());
    assert!(matches!(panel.visible_rows()[0], HistoryRow::Agent { .. }));
}

#[test]
fn visual_fixtures() {
    let Some(directory) = std::env::var_os("PIKO_HISTORY_QA_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut panel = panel();
    panel.work.as_mut().unwrap().items[2].provenance = HistoryProvenance::Diagnostic;
    panel.work.as_mut().unwrap().items[2].availability = HistoryAvailability::Unavailable {
        reason: "capture absent".into(),
    };
    for lens in HistoryLens::ALL {
        panel.lens = lens;
        if lens == HistoryLens::Agents {
            panel.work = None;
        }
        if lens == HistoryLens::Transcript {
            panel.transcript = Some(HistoryTranscriptPage {
                session_id: "session-history-fixture".into(),
                revision: 12,
                next_cursor: None,
                items: (0..12)
                    .map(|i| HistoryTranscriptItem {
                        item_ref: item(i).item_ref,
                        kind: HistoryItemKind::new("message"),
                        depth: i as u32,
                        agent_instance_id: Some("agent-main".into()),
                        parent_id: Some(format!("message-{i}")),
                        root_input_id: Some("input-inspect-history".into()),
                        model_step_id: None,
                        summary: format!("Message {i} · branch content / 分支内容"),
                        selected: i == 5,
                        off_branch: i > 5,
                        has_detail: true,
                    })
                    .collect(),
            });
        }
        if lens == HistoryLens::Journal {
            panel.journal = Some(HistoryJournalPage {
                session_id: "session-history-fixture".into(),
                revision: 12,
                next_cursor: None,
                commits: (0..5)
                    .map(|i| HistoryCommitSummary {
                        revision: i as u64 + 1,
                        commit_id: format!("commit-{i}"),
                        producer: "hostd".into(),
                        committed_at: 0,
                        causation_id: Some("input-inspect-history".into()),
                        correlation_id: None,
                        events: vec![item(i)],
                    })
                    .collect(),
            });
        }
        panel.clear_detail();
        for width in [40, 60, 120] {
            export_frame(
                &panel,
                &directory,
                &format!("{}-{width}", lens.index()),
                width,
            );
        }
    }
    let mut detail_panel = self::panel();
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
fn summary_inspection_is_available_in_compact_mode_without_a_request() {
    let mut panel = panel();
    panel.back();
    panel.select_lens(1);
    panel.inspect_summary();
    let buffer = render(&panel, 40, 24);
    assert!(panel.shows_detail_only());
    assert!(shown(&buffer).contains("agent-main"));
    assert!(panel.pending_command_id.is_none());
    assert!(panel.detail.is_none());
    assert!(!panel.back());
    assert_eq!(panel.active_pane, PaneSide::First);
}

#[test]
fn wheel_over_list_keeps_detail_position_and_wheel_over_detail_keeps_selection() {
    use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};
    let mut panel = panel();
    panel.selected = 10;
    open_detail(&mut panel);
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
    assert_eq!(panel.detail_viewport.get().top(), 1);
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
    assert_eq!(panel.selected, 11);
    assert_eq!(panel.detail_viewport.get().top(), 1);
    assert!(
        matches!(panel.opened_row, Some(HistoryRow::Item { ref item, .. }) if item.item_ref.token == "opaque-10")
    );
}

#[test]
fn filtered_count_retains_loaded_scope() {
    let mut panel = panel();
    panel.filter = "Step 12 ·".into();
    assert_eq!(panel.row_count(), 1);
    assert_eq!(panel.loaded_row_count(), 30);
    let buffer = render(&panel, 60, 24);
    assert!(shown(&buffer).contains("1 / 30 loaded"));
    assert!(shown(&buffer).contains("more"));
}

#[test]
fn compact_detail_feedback_wraps_and_identifies_the_opened_item() {
    let mut panel = panel();
    panel.selected = 10;
    open_detail(&mut panel);
    panel.detail_error = Some("Transport failed while fetching the recorded body".into());
    let buffer = render(&panel, 40, 40);
    let text = shown(&buffer);
    assert!(text.contains("step-10"));
    let words: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '│')
        .collect();
    assert!(words.contains("openagaintoretry"), "{words}");
    panel.detail_error = None;
    panel.detail_loading = true;
    let buffer = render(&panel, 40, 40);
    assert!(shown(&buffer).contains("step-10"));
    assert!(shown(&buffer).contains("Loading selected detail"));
}

#[test]
fn summary_only_rows_do_not_offer_unavailable_body_actions() {
    let mut panel = panel();
    panel.back();
    panel.select_lens(1);
    panel.inspect_summary();
    let buffer = render(&panel, 60, 30);
    let text = shown(&buffer);
    assert!(text.contains("Back returns to the list"));
    assert!(!text.contains("Open the item"));
}

#[test]
fn compact_lens_tabs_keep_complete_names_when_they_fit() {
    let panel = panel();
    let buffer = render(&panel, 40, 24);
    let text = shown(&buffer);
    assert!(text.contains("Transcript"));
    assert!(text.contains("Journal"));
    let hits = panel.painted_regions.borrow();
    for (rect, _) in hits.iter().filter(|(_, hit)| matches!(hit, HitId::Mode(_))) {
        assert!(rect.right() < 40);
    }
}
