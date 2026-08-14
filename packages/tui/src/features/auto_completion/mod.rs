use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::path::Path;

use crate::app::{
    HitId,
    command::{EditorAction, TuiCommandEntry},
};
use crate::ui::components::selectable_list::{
    ColumnCell, SelectableItem, SelectableList, SelectablePanelBody, paint_index_hover,
    paint_selectable_panel, selectable_row_regions,
};
use crate::ui::components::{NO_MATCHES, pane::PaneSpec, pane::PaneTitleAffix};
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};
use crate::ui::interaction_hints::InteractionHints;

pub mod file_browser;
pub mod provider;
pub mod slash_commands;

use file_browser::FileBrowserProvider;
use provider::AutoCompleteProvider;
use slash_commands::SlashCommandProvider;

/// One completion suggestion (domain payload + column cells for paint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRow {
    pub replacement: String,
    pub start: usize,
    pub end: usize,
    pub cells: Vec<ColumnCell>,
    pub keep_active: bool,
    /// Enter executes the accepted editor text instead of only inserting it.
    pub submit_on_accept: bool,
}

pub struct AutoComplete {
    pub active: bool,
    pub list: SelectableList<CompletionRow>,
    pub active_provider_idx: Option<usize>,
    pub providers: Vec<Box<dyn AutoCompleteProvider>>,
}

impl Default for AutoComplete {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoComplete {
    pub fn new() -> Self {
        Self {
            active: false,
            list: SelectableList::new(Vec::new()),
            active_provider_idx: None,
            providers: vec![
                Box::new(SlashCommandProvider),
                Box::new(FileBrowserProvider),
            ],
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn interaction_hints(&self) -> InteractionHints<'static> {
        self.active_provider_idx
            .map(|idx| self.providers[idx].hints())
            .unwrap_or_else(|| InteractionHints::new("Esc cancel"))
    }

    pub fn select_next(&mut self) {
        self.list.select_next_wrapped();
    }

    pub fn select_prev(&mut self) {
        self.list.select_prev_wrapped();
    }

    /// Select the suggestion at `idx` (pointer clicks), clamped to the list.
    pub fn select_index(&mut self, idx: usize) {
        self.list.select_index(idx);
    }

    /// Accepts the currently selected completion item.
    /// Clears selection and deactivates if keep_active is false.
    pub fn accept(&mut self) -> Option<CompletionRow> {
        let item = self.list.selected_item().cloned();
        if item.as_ref().is_some_and(|i| !i.keep_active) {
            self.clear();
        }
        item
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.list.clear();
        self.active_provider_idx = None;
    }

    fn pane_spec(&self) -> PaneSpec<'static> {
        let label = if let Some(idx) = self.active_provider_idx {
            self.providers[idx].label()
        } else {
            "suggestions"
        };
        let total = self.list.len();
        let selected_one = usize::from(total > 0).saturating_mul(self.list.selected + 1);
        PaneSpec::minimal(label)
            .no_search()
            .affix(PaneTitleAffix::selection(selected_one, total))
            .focused(true)
    }

    fn display_items(&self) -> Vec<SelectableItem> {
        self.list
            .items
            .iter()
            .map(|row| SelectableItem::columns(row.cells.clone()))
            .collect()
    }

    fn row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        selectable_row_regions(
            area,
            &self.pane_spec(),
            &self.display_items(),
            self.list.selected,
            "",
        )
    }

    /// Paint-aligned pointer rows using stable source indices from the shared list viewport.
    pub(crate) fn pointer_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.row_regions(area)
            .into_iter()
            .map(|(rect, index)| (rect, HitId::Suggest(index)))
            .collect()
    }

    /// Updates completions state based on current editor text and cursor.
    pub fn update(&mut self, cwd: &Path, commands: &[TuiCommandEntry], text: &str, cursor: usize) {
        let matched_idx = self
            .providers
            .iter()
            .position(|provider| provider.is_triggered(text, cursor));

        self.active_provider_idx = matched_idx;
        self.active = matched_idx.is_some();

        let mut items = if let Some(idx) = matched_idx {
            self.providers[idx].update(cwd, commands, text, cursor)
        } else {
            Vec::new()
        };

        // Safety limit to avoid performance issues
        items.truncate(100);

        let prev = self.list.selected;
        self.list = SelectableList::new(items);
        self.list.selected = prev.min(self.list.len().saturating_sub(1));
    }

    /// Renders the completions list in the allocated area (Minimal pane, no search).
    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &crate::theme::Theme,
        interaction: InteractionState<HitId>,
    ) {
        let spec = self.pane_spec();
        let items = self.display_items();

        let body = if items.is_empty() {
            SelectablePanelBody::Message(Paragraph::new(Line::from(vec![Span::styled(
                format!("  {NO_MATCHES}"),
                Style::default().fg(theme.dim),
            )])))
        } else {
            SelectablePanelBody::Columns {
                items: &items,
                selected: self.list.selected,
                widths: None,
            }
        };

        let _ = paint_selectable_panel(frame, area, theme, &spec, body);
        let hovered = match interaction.hovered {
            Some(HitId::Suggest(index)) => Some(index),
            _ => None,
        };
        paint_index_hover(
            frame,
            &self.row_regions(area),
            hovered,
            self.list.selected,
            theme,
        );
    }
}

impl PointerComponent<HitId> for AutoComplete {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Suggest(index))) if index < self.list.len() => {
                // Same as Enter: insert, then submit when the row is Immediate
                // (slash command palette). File completions keep submit_on_accept=false.
                self.select_index(index);
                vec![EditorAction::AcceptAndSubmitSuggestion.into()]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::{CommandTarget, LocalCommandId};
    use piko_protocol::HostCommandInvoke;

    fn rows(count: usize) -> Vec<CompletionRow> {
        (0..count)
            .map(|index| CompletionRow {
                replacement: format!("item-{index}"),
                start: 0,
                end: 0,
                cells: vec![ColumnCell::primary(format!("item-{index}"))],
                keep_active: true,
                submit_on_accept: false,
            })
            .collect()
    }

    fn commands() -> Vec<TuiCommandEntry> {
        vec![TuiCommandEntry {
            slash: "/resume".to_string(),
            title: "Sessions".to_string(),
            detail: "list and open sessions".to_string(),
            invoke: HostCommandInvoke::Immediate,
            target: CommandTarget::Local(LocalCommandId::Sessions),
        }]
    }

    #[test]
    fn slash_trigger_stays_active_with_no_matches() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/zzz", 4);
        assert!(ac.active);
        assert!(ac.list.is_empty());
    }

    #[test]
    fn slash_completion_uses_command_token_range() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/res", 4);
        assert!(ac.active);
        let resume = ac
            .list
            .items
            .iter()
            .find(|item| item.cells[0].text == "/resume")
            .unwrap();
        assert_eq!(resume.start, 0);
        assert_eq!(resume.end, 4);
        assert_eq!(resume.replacement, "/resume ");
    }

    #[test]
    fn slash_trigger_inactive_in_arguments() {
        let mut ac = AutoComplete::new();
        ac.update(Path::new("."), &commands(), "/resume now", 8);
        assert!(!ac.active);
    }

    #[test]
    fn pointer_regions_follow_selected_row_beyond_first_viewport() {
        let mut ac = AutoComplete::new();
        ac.list = SelectableList::new(rows(10));
        ac.list.selected = 8;

        let indices: Vec<_> = ac
            .pointer_regions(Rect::new(0, 0, 40, 9))
            .into_iter()
            .filter_map(|(_, hit)| match hit {
                HitId::Suggest(index) => Some(index),
                _ => None,
            })
            .collect();

        assert_eq!(indices, vec![2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn wheel_moves_selection_without_accepting_suggestion() {
        let mut ac = AutoComplete::new();
        ac.list = SelectableList::new(rows(3));
        let hit = ComponentHit {
            element: Some(HitId::Suggest(0)),
            rect: Rect::new(0, 0, 40, 1),
            x: 0,
            y: 0,
        };

        assert!(ac.pointer_event(hit, PointerGesture::ScrollDown).is_empty());
        assert_eq!(ac.list.selected, 1);
        assert!(ac.pointer_event(hit, PointerGesture::ScrollUp).is_empty());
        assert_eq!(ac.list.selected, 0);
    }
}
