use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
};

/// A node in the hierarchical menu tree.
#[derive(Clone, Debug)]
pub enum MenuNode<T: Clone> {
    Group {
        title: String,
        detail: String,
        children: Vec<MenuNode<T>>,
    },
    Action {
        title: String,
        detail: String,
        action: T,
    },
}

impl<T: Clone> MenuNode<T> {
    pub fn title(&self) -> &str {
        match self {
            Self::Group { title, .. } => title,
            Self::Action { title, .. } => title,
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Group { detail, .. } => detail,
            Self::Action { detail, .. } => detail,
        }
    }
}

/// A single frame representing the current level in the navigation stack.
pub struct MenuFrame<T: Clone> {
    pub title: String,
    pub list: FilterableList<MenuNode<T>>,
}

/// Result returned when confirming a menu selection.
pub enum MenuConfirmResult<T> {
    SubMenuPushed,
    Action(T, String),
    None,
}

/// A generic keyboard-navigable hierarchical menu component.
pub struct HierarchicalMenu<T: Clone> {
    pub stack: Vec<MenuFrame<T>>,
}

impl<T: Clone> HierarchicalMenu<T> {
    pub fn new(root: MenuNode<T>) -> Self {
        let mut menu = Self { stack: Vec::new() };
        menu.open(root);
        menu
    }

    /// Reset the stack to contain only the specified root menu node.
    pub fn open(&mut self, node: MenuNode<T>) {
        self.stack.clear();
        self.push_node(node);
    }

    /// Push a group menu node onto the navigation stack.
    pub fn push_node(&mut self, node: MenuNode<T>) {
        if let MenuNode::Group { title, children, .. } = node {
            self.stack.push(MenuFrame {
                title,
                list: FilterableList::new(children),
            });
        }
    }

    /// Pop the top frame off the navigation stack.
    /// Returns true if frames still remain, false if the stack is now empty.
    pub fn pop(&mut self) -> bool {
        self.stack.pop();
        !self.stack.is_empty()
    }

    pub fn select_next(&mut self, filter: &str) {
        if let Some(frame) = self.stack.last_mut() {
            frame.list.select_next(filter, |item| {
                item.title().to_lowercase().contains(filter)
                    || item.detail().to_lowercase().contains(filter)
            });
        }
    }

    pub fn select_prev(&mut self, filter: &str) {
        if let Some(frame) = self.stack.last_mut() {
            frame.list.select_prev(filter, |item| {
                item.title().to_lowercase().contains(filter)
                    || item.detail().to_lowercase().contains(filter)
            });
        }
    }

    /// Confirm the currently selected option.
    pub fn confirm(&mut self, filter_text: &mut String) -> MenuConfirmResult<T> {
        let Some(frame) = self.stack.last() else {
            return MenuConfirmResult::None;
        };

        let filter_str = filter_text.as_str();
        let filtered = frame.list.filtered_indices(filter_str, |item| {
            item.title().to_lowercase().contains(filter_str)
                || item.detail().to_lowercase().contains(filter_str)
        });
        if filtered.is_empty() {
            return MenuConfirmResult::None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == frame.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        let Some(&orig_idx) = filtered.get(selected_filtered_idx) else {
            return MenuConfirmResult::None;
        };
        let selected_item = frame.list.items[orig_idx].clone();

        match selected_item {
            MenuNode::Group { .. } => {
                self.push_node(selected_item);
                filter_text.clear();
                MenuConfirmResult::SubMenuPushed
            }
            MenuNode::Action { action, title, .. } => {
                MenuConfirmResult::Action(action, title)
            }
        }
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        is_active_fn: impl Fn(&T) -> bool,
        theme: &Theme,
    ) {
        let Some(current_frame) = self.stack.last() else {
            return;
        };

        let items: Vec<FilterableItem> = current_frame
            .list
            .items
            .iter()
            .map(|item| {
                let is_active = match item {
                    MenuNode::Action { action, .. } => is_active_fn(action),
                    MenuNode::Group { .. } => false,
                };

                let suffix = match item {
                    MenuNode::Group { .. } => " >",
                    MenuNode::Action { .. } => "",
                };

                FilterableItem {
                    primary: format!("{}{}", item.title(), suffix),
                    detail: item.detail().to_string(),
                    is_active,
                }
            })
            .collect();

        render_filterable_list(
            frame,
            area,
            &current_frame.title,
            &items,
            current_frame.list.selected,
            filter,
            theme,
        );
    }
}
