use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem},
};

use super::selector_item;

/// A discovered model option.
#[derive(Clone)]
pub struct ModelOption {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub has_auth: bool,
}

/// Models list overlay.
pub struct ModelsOverlay {
    pub models: Vec<ModelOption>,
    pub selected: usize,
}

impl ModelsOverlay {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            selected: 0,
        }
    }

    pub fn load(&mut self, models: Vec<ModelOption>) {
        self.models = models;
        self.selected = self.selected.min(self.models.len().saturating_sub(1));
    }

    pub fn select_next(&mut self) {
        if !self.models.is_empty() {
            self.selected = (self.selected + 1).min(self.models.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_model(&self) -> Option<&ModelOption> {
        self.models.get(self.selected)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let items: Vec<ListItem<'_>> = if self.models.is_empty() {
            vec![ListItem::new(Line::from("No models returned by hostd."))]
        } else {
            self.models
                .iter()
                .enumerate()
                .map(|(idx, model)| model_item(idx == self.selected, model))
                .collect()
        };
        let widget = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("models | j/k select | Enter set default | Esc close"),
        );
        frame.render_widget(widget, area);
    }
}

fn model_item(selected: bool, model: &ModelOption) -> ListItem<'_> {
    let auth = if model.has_auth { "auth" } else { "no auth" };
    selector_item(
        selected,
        format!("{}/{}", model.provider, model.id),
        format!("{}  {auth}", model.name),
    )
}
