//! One active context menu and focus origin per GPUI window.

use std::collections::HashMap;

use gpui::{EntityId, FocusHandle, Global, WeakEntity, WindowId};

use super::ContextMenu;

pub(crate) struct ActiveContextMenu {
    pub(crate) entity_id: EntityId,
    pub(crate) menu: WeakEntity<ContextMenu>,
    pub(crate) restore_focus: Option<FocusHandle>,
}

#[derive(Default)]
pub(crate) struct ContextMenuRegistry {
    pub(crate) windows: HashMap<WindowId, ActiveContextMenu>,
}

impl Global for ContextMenuRegistry {}
