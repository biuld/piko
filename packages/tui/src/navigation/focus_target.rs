//! LIFO focus targets for piko-tui.

use super::SurfaceId;

/// Focus stack entry for the product client.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AppMode {
    /// Editor base (stack bottom).
    Chat,
    Surface(SurfaceId),
}

impl AppMode {
    pub fn as_surface(self) -> Option<SurfaceId> {
        match self {
            Self::Surface(s) => Some(s),
            Self::Chat => None,
        }
    }

    pub fn is_surface(self, surface: SurfaceId) -> bool {
        self.as_surface() == Some(surface)
    }

    pub fn from_surface(surface: SurfaceId) -> Self {
        Self::Surface(surface)
    }

    pub fn is_editor_base(self) -> bool {
        self == Self::Chat
    }
}
