//! Immutable flat context-menu item specifications.

use std::rc::Rc;

use gpui::{App, SharedString, Window};

pub(crate) type ContextMenuCallback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Semantic text treatment for an action item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContextMenuItemTone {
    #[default]
    Default,
    Destructive,
}

/// One action or separator in a flat context menu.
pub struct ContextMenuItem {
    pub(crate) kind: ContextMenuItemKind,
}

pub(crate) enum ContextMenuItemKind {
    Action {
        label: SharedString,
        enabled: bool,
        tone: ContextMenuItemTone,
        callback: ContextMenuCallback,
    },
    Separator,
}

impl ContextMenuItem {
    pub fn action(
        label: impl Into<SharedString>,
        callback: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            kind: ContextMenuItemKind::Action {
                label: label.into(),
                enabled: true,
                tone: ContextMenuItemTone::Default,
                callback: Rc::new(callback),
            },
        }
    }

    pub fn separator() -> Self {
        Self {
            kind: ContextMenuItemKind::Separator,
        }
    }

    #[must_use]
    pub fn enabled(mut self, value: bool) -> Self {
        if let ContextMenuItemKind::Action { enabled, .. } = &mut self.kind {
            *enabled = value;
        }
        self
    }

    #[must_use]
    pub fn tone(mut self, value: ContextMenuItemTone) -> Self {
        if let ContextMenuItemKind::Action { tone, .. } = &mut self.kind {
            *tone = value;
        }
        self
    }

    pub(crate) fn selectable(&self) -> bool {
        matches!(self.kind, ContextMenuItemKind::Action { enabled: true, .. })
    }

    pub(crate) fn callback(&self) -> Option<ContextMenuCallback> {
        match &self.kind {
            ContextMenuItemKind::Action {
                enabled: true,
                callback,
                ..
            } => Some(callback.clone()),
            ContextMenuItemKind::Action { .. } | ContextMenuItemKind::Separator => None,
        }
    }
}

/// Contents of one flat context menu.
#[derive(Default)]
pub struct ContextMenuSpec {
    pub(crate) items: Vec<ContextMenuItem>,
}

impl ContextMenuSpec {
    pub fn new(items: impl IntoIterator<Item = ContextMenuItem>) -> Self {
        let mut items = items.into_iter().collect::<Vec<_>>();
        while items
            .first()
            .is_some_and(|item| matches!(item.kind, ContextMenuItemKind::Separator))
        {
            items.remove(0);
        }
        while items
            .last()
            .is_some_and(|item| matches!(item.kind, ContextMenuItemKind::Separator))
        {
            items.pop();
        }
        Self { items }
    }

    pub fn is_empty(&self) -> bool {
        !self.items.iter().any(ContextMenuItem::selectable)
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextMenuItem, ContextMenuSpec};

    #[test]
    fn spec_trims_edge_separators_and_requires_enabled_action() {
        let spec = ContextMenuSpec::new([
            ContextMenuItem::separator(),
            ContextMenuItem::action("Disabled", |_, _| {}).enabled(false),
            ContextMenuItem::separator(),
        ]);
        assert_eq!(spec.items.len(), 1);
        assert!(spec.is_empty());
    }
}
