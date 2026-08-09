use piko_protocol::command::ProcessInfo;
use piko_tui_layout::{Component, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use crate::{
    app::HitId,
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::components::{
        pane::PaneSpec,
        selectable_list::{
            ColumnAlign, ColumnCell, SelectableItem, SelectableList, SelectablePanelBody,
            paint_selectable_panel,
        },
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
        let items: Vec<SelectableItem> = self
            .processes
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
            .collect();
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

    fn component_regions(&self, _area: Rect) -> Vec<(Rect, HitId)> {
        Vec::new()
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
}
