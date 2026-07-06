use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::hierarchical_menu::{HierarchicalMenu, MenuConfirmResult, MenuNode},
};

/// A discovered model option.
#[derive(Clone, Debug)]
pub struct ModelOption {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub has_auth: bool,
}

/// Model selector panel: encapsulates a generic HierarchicalMenu.
pub struct ModelSelector {
    pub menu: HierarchicalMenu<ModelOption>,
    pub filter: String,
}

impl ModelSelector {
    pub fn new() -> Self {
        let root = MenuNode::Group {
            title: "models".to_string(),
            detail: "List and set default model".to_string(),
            children: Vec::new(),
        };
        Self {
            menu: HierarchicalMenu::new(root),
            filter: String::new(),
        }
    }

    /// Load dynamic models retrieved from hostd into the hierarchical menu tree.
    pub fn load(&mut self, models: Vec<ModelOption>) {
        let children = models
            .into_iter()
            .map(|item| {
                let auth = if item.has_auth { "auth" } else { "no auth" };
                let title = format!("{}/{}", item.provider, item.id);
                let detail = format!("{}  {}", item.name, auth);
                MenuNode::Action {
                    title,
                    detail,
                    action: item,
                }
            })
            .collect();

        let root = MenuNode::Group {
            title: "models".to_string(),
            detail: "List and set default model".to_string(),
            children,
        };
        self.menu.open(root);
    }

    pub fn len(&self) -> usize {
        self.menu
            .stack
            .last()
            .map(|frame| frame.list.items.len())
            .unwrap_or(0)
    }

    pub fn reset(&mut self) {
        if let Some(frame) = self.menu.stack.last_mut() {
            frame.list.selected = 0;
        }
    }

    pub fn select_next(&mut self) {
        self.menu.select_next(&self.filter);
    }

    pub fn select_prev(&mut self) {
        self.menu.select_prev(&self.filter);
    }

    pub fn confirm(&mut self) -> MenuConfirmResult<ModelOption> {
        self.menu.confirm(&mut self.filter)
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        active_model_id: Option<&str>,
        theme: &Theme,
    ) {
        self.menu.render(
            frame,
            area,
            &self.filter,
            |model| {
                let model_id_full = format!("{}/{}", model.provider, model.id);
                active_model_id
                    .map(|id| id == model_id_full)
                    .unwrap_or(false)
            },
            theme,
        );
    }
}
