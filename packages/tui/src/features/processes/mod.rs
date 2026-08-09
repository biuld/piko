use piko_protocol::command::ProcessInfo;
use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use crate::{
    app::{HitId, command::SurfaceAction},
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::{
        components::{
            pane::PaneSpec,
            selectable_list::{
                ColumnAlign, ColumnCell, SelectableItem, SelectableList, SelectablePanelBody,
                paint_row_hover, paint_selectable_panel, selectable_row_regions,
            },
        },
        interaction::{ComponentHit, PointerComponent, PointerGesture},
    },
};

#[derive(Default)]
pub struct ProcessPanel {
    processes: SelectableList<ProcessInfo>,
    confirming_process_id: Option<String>,
}

pub enum ProcessConfirm {
    None,
    Armed(String),
    Confirmed(String),
    NotRunning(String),
}

impl ProcessPanel {
    fn display_items(&self) -> Vec<SelectableItem> {
        self.processes
            .items
            .iter()
            .map(|process| {
                let state = if process.exited {
                    process
                        .exit_code
                        .map(|code| format!("exit {code}"))
                        .or_else(|| process.signal.map(|signal| format!("signal {signal}")))
                        .unwrap_or_else(|| "exited".into())
                } else {
                    "running".into()
                };
                let item = SelectableItem::columns([
                    ColumnCell::primary(process.process_id.clone()),
                    ColumnCell::secondary(process.pid.to_string()).align(ColumnAlign::Right),
                    ColumnCell::secondary(process.command.clone()),
                    ColumnCell::secondary(state),
                    ColumnCell::secondary(process.cwd.clone()),
                ]);
                if self.confirming_process_id.as_deref() == Some(process.process_id.as_str()) {
                    item.badge("confirm stop")
                } else {
                    item
                }
            })
            .collect()
    }

    fn row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        let title = self
            .confirming_process_id
            .as_ref()
            .map(|id| format!("stop {id}?"));
        let title = title.as_deref().unwrap_or("processes");
        let hints = if self.confirming_process_id.is_some() {
            "Enter confirm stop · Esc cancel"
        } else {
            "↑/↓ browse · Enter stop · Esc close"
        };
        let spec = PaneSpec::new(title).no_search().hints(hints).focused(true);
        selectable_row_regions(
            area,
            &spec,
            &self.display_items(),
            self.processes.selected,
            "",
        )
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_processes(&mut self, processes: Vec<ProcessInfo>) {
        self.processes = SelectableList::new(processes);
        self.confirming_process_id = None;
    }

    pub fn select_band_budget(&self) -> SelectBandBudget {
        SelectBandBudget::standard_info(self.processes.len().clamp(1, 10) as u16)
    }

    pub fn select_next(&mut self) {
        self.confirming_process_id = None;
        self.processes.select_next("", |_| true);
    }

    pub fn select_prev(&mut self) {
        self.confirming_process_id = None;
        self.processes.select_prev("", |_| true);
    }

    pub fn confirm_stop(&mut self) -> ProcessConfirm {
        let Some(process) = self.processes.selected_item() else {
            return ProcessConfirm::None;
        };
        if process.exited {
            return ProcessConfirm::NotRunning(process.process_id.clone());
        }
        if self.confirming_process_id.as_deref() == Some(process.process_id.as_str()) {
            self.confirming_process_id = None;
            ProcessConfirm::Confirmed(process.process_id.clone())
        } else {
            self.confirming_process_id = Some(process.process_id.clone());
            ProcessConfirm::Armed(process.process_id.clone())
        }
    }

    pub fn cancel_confirmation(&mut self) -> bool {
        self.confirming_process_id.take().is_some()
    }

    pub fn remove(&mut self, process_id: &str) {
        self.processes
            .items
            .retain(|process| process.process_id != process_id);
        self.processes.selected = self
            .processes
            .selected
            .min(self.processes.len().saturating_sub(1));
        self.confirming_process_id = None;
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let title = self
            .confirming_process_id
            .as_ref()
            .map(|id| format!("stop {id}?"))
            .unwrap_or_else(|| "processes".into());
        let hints = if self.confirming_process_id.is_some() {
            "Enter confirm stop · Esc cancel"
        } else {
            "↑/↓ browse · Enter stop · Esc close"
        };
        let spec = PaneSpec::new(&title).no_search().hints(hints).focused(true);
        let items = self.display_items();
        let body = if items.is_empty() {
            SelectablePanelBody::Message(ratatui::widgets::Paragraph::new(
                "No external processes are running.",
            ))
        } else {
            SelectablePanelBody::Columns {
                items: &items,
                selected: self.processes.selected,
                widths: None,
            }
        };
        let _ = paint_selectable_panel(frame, area, theme, &spec, body);
    }
}

impl Component<HitId, Theme> for ProcessPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx);
        let regions = self.row_regions(area);
        paint_row_hover(frame, &regions, interaction, self.processes.selected, ctx);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.row_regions(area)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect()
    }
}

impl PointerComponent<HitId> for ProcessPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) if i < self.processes.len() => {
                if self.processes.selected != i {
                    self.confirming_process_id = None;
                }
                self.processes.selected = i;
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev();
                Vec::new()
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl SurfacePanel<SurfaceId, HitId, Theme> for ProcessPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Processes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(id: &str, pid: u32) -> ProcessInfo {
        ProcessInfo {
            process_id: id.into(),
            pid,
            command: "cargo test".into(),
            cwd: "/tmp/project".into(),
            exited: false,
            exit_code: None,
            signal: None,
        }
    }

    #[test]
    fn process_rows_use_shared_selection_state() {
        let mut panel = ProcessPanel::new();
        panel.set_processes(vec![process("one", 1), process("two", 2)]);
        panel.select_next();
        assert_eq!(panel.processes.selected, 1);
        panel.select_prev();
        assert_eq!(panel.processes.selected, 0);
    }

    #[test]
    fn stopping_a_process_requires_two_confirms() {
        let mut panel = ProcessPanel::new();
        panel.set_processes(vec![process("one", 1)]);
        assert!(matches!(
            panel.confirm_stop(),
            ProcessConfirm::Armed(id) if id == "one"
        ));
        assert!(matches!(
            panel.confirm_stop(),
            ProcessConfirm::Confirmed(id) if id == "one"
        ));
    }

    #[test]
    fn pointer_click_preserves_two_stage_stop_confirmation() {
        let mut panel = ProcessPanel::new();
        panel.set_processes(vec![process("one", 1), process("two", 2)]);
        let hit = ComponentHit {
            element: Some(HitId::Row(1)),
            rect: Rect::new(0, 0, 10, 1),
            x: 0,
            y: 0,
        };
        let first = panel.pointer_event(hit, PointerGesture::Activate);
        assert!(matches!(
            first.as_slice(),
            [crate::app::command::Action::Surface(SurfaceAction::Confirm)]
        ));
        assert!(matches!(panel.confirm_stop(), ProcessConfirm::Armed(id) if id == "two"));
        let second = panel.pointer_event(hit, PointerGesture::Activate);
        assert!(matches!(
            second.as_slice(),
            [crate::app::command::Action::Surface(SurfaceAction::Confirm)]
        ));
        assert!(matches!(panel.confirm_stop(), ProcessConfirm::Confirmed(id) if id == "two"));
    }
}
