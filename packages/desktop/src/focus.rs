//! Product focus ownership and temporary-layer policy.

use island::components::overlay::OverlayHost;
use piko_client_core::timeline::ToolStatus;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChipDetail {
    /// Full thinking text captured when the chip was clicked.
    Thinking { segment_id: String, text: String },
    /// Tool identity; payloads re-resolve from the projection on paint.
    Tool {
        call_id: String,
        name: String,
        status: ToolStatus,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusOwner {
    Sidebar,
    AgentTabs,
    Timeline,
    Composer,
}

impl FocusOwner {
    pub fn next(self) -> Self {
        match self {
            Self::Timeline => Self::Composer,
            Self::Composer => Self::Sidebar,
            Self::Sidebar => Self::AgentTabs,
            Self::AgentTabs => Self::Timeline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Attention,
    Settings,
    ChipDetail,
}

#[derive(Debug, Default)]
pub struct TemporaryLayers {
    host: OverlayHost<LayerKind>,
    restore: Option<FocusOwner>,
}

impl TemporaryLayers {
    pub fn active(&self) -> Option<LayerKind> {
        self.host.active()
    }

    pub fn open(&mut self, kind: LayerKind, initiating: FocusOwner) {
        if self.host.try_open(kind) && self.host.begin_focus_session() {
            self.restore = Some(initiating);
        }
    }

    pub fn close(&mut self) -> Option<FocusOwner> {
        if !self.host.close() {
            return None;
        }
        let _ = self.host.end_focus_session_if_idle();
        self.restore.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_restores_the_initiating_surface() {
        let mut layers = TemporaryLayers::default();
        layers.open(LayerKind::Settings, FocusOwner::Composer);
        assert_eq!(layers.active(), Some(LayerKind::Settings));
        assert_eq!(layers.close(), Some(FocusOwner::Composer));
        assert_eq!(layers.active(), None);
    }

    #[test]
    fn escape_when_idle_is_a_no_op() {
        assert_eq!(TemporaryLayers::default().close(), None);
    }

    #[test]
    fn replacing_a_layer_keeps_the_original_restore_owner() {
        let mut layers = TemporaryLayers::default();
        layers.open(LayerKind::Settings, FocusOwner::Timeline);
        layers.open(LayerKind::Attention, FocusOwner::Composer);
        assert_eq!(layers.active(), Some(LayerKind::Attention));
        assert_eq!(layers.close(), Some(FocusOwner::Timeline));
    }

    #[test]
    fn traversal_reaches_every_primary_surface() {
        assert_eq!(FocusOwner::Timeline.next(), FocusOwner::Composer);
        assert_eq!(FocusOwner::Composer.next(), FocusOwner::Sidebar);
        assert_eq!(FocusOwner::Sidebar.next(), FocusOwner::AgentTabs);
        assert_eq!(FocusOwner::AgentTabs.next(), FocusOwner::Timeline);
    }
}
