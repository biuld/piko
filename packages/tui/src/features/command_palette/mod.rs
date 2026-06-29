use ratatui::{Frame, layout::Rect};

use piko_protocol::{CommandCatalogAction, CommandCatalogItem};

use crate::{
    theme::Theme,
    ui::components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
};

/// A single item in the command palette.
#[derive(Clone)]
pub struct CommandItem {
    pub title: String,
    pub detail: String,
    pub action: CommandCatalogAction,
}

/// Command palette panel: static list of commands with selection state.
pub struct CommandPalette {
    pub list: FilterableList<CommandItem>,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
        }
    }

    pub fn load(&mut self, commands: &[CommandCatalogItem]) {
        self.list.items = commands
            .iter()
            .filter(|command| command.visible_in_palette)
            .map(|command| CommandItem {
                title: command.title.clone(),
                detail: command.detail.clone(),
                action: command.action.clone(),
            })
            .collect();
        self.list.selected = 0;
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn selected_action(&self, filter: &str) -> Option<CommandCatalogAction> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
            .map(|item| item.action.clone())
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, filter: &str, theme: &Theme) {
        let items: Vec<FilterableItem> = self
            .list
            .items
            .iter()
            .map(|item| FilterableItem {
                primary: item.title.clone(),
                detail: item.detail.clone(),
                is_active: false,
            })
            .collect();
        render_filterable_list(
            frame,
            area,
            "commands",
            &items,
            self.list.selected,
            filter,
            theme,
        );
    }
}
