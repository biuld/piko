//! Product surface catalog and modal placement policy.
//!
//! Intent split (z-stack only — plane is never rebuilt for a surface):
//! - **Browse** → [`ModalPlacement::CoverBody`]
//! - **Select** → [`ModalPlacement::ComposerBand`]
//! - **Dock** → [`ModalPlacement::ComposerBand`] (approval and tool
//!   interaction replace the composer)
//! - **Modal** → [`ModalPlacement::Centered`] (settings dialog)

use piko_tui_layout::{ModalLayer, ModalPlacement, cells_from_percent, leaf};
use ratatui::layout::Rect;

use super::Region;

/// Overlay surfaces (focus identity + modal content).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceId {
    /// Session agent list — switch viewed agent (Select / ComposerBand).
    Agents,
    Sessions,
    Tree,
    Models,
    Thinking,
    Settings,
    Status,
    Mcp,
    Diagnostics,
    Approval,
    ToolInteraction,
    SummaryPrompt,
    AuthSelector,
}

/// How a surface is intended to mount (product policy).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceIntent {
    /// Explore / configure / blocking decision: covers the body above chrome.
    Browse,
    /// Pick a value near the composer.
    Select,
    /// Blocking prompt that replaces the composer dock (approval, tool
    /// interaction).
    Dock,
    /// Centered dialog (settings).
    Modal,
}

impl SurfaceId {
    pub fn intent(self) -> SurfaceIntent {
        match self {
            Self::Sessions
            | Self::Tree
            | Self::Status
            | Self::Diagnostics
            | Self::SummaryPrompt => SurfaceIntent::Browse,

            // Session agent picker (viewed-agent switch) sits near the composer,
            // same as Models / Thinking / Auth.
            Self::Agents | Self::Mcp | Self::Models | Self::Thinking | Self::AuthSelector => {
                SurfaceIntent::Select
            }

            Self::Approval | Self::ToolInteraction => SurfaceIntent::Dock,

            Self::Settings => SurfaceIntent::Modal,
        }
    }

    pub fn modal_placement(self, body: Rect, centered_size: Option<(u16, u16)>) -> ModalPlacement {
        match self.intent() {
            SurfaceIntent::Browse => ModalPlacement::CoverBody,
            SurfaceIntent::Select | SurfaceIntent::Dock => ModalPlacement::ComposerBand,
            SurfaceIntent::Modal => ModalPlacement::Centered {
                max_width: centered_size
                    .map(|(w, _)| w)
                    .unwrap_or_else(|| cells_from_percent(body.width, 88).max(40)),
                max_height: centered_size
                    .map(|(_, h)| h)
                    .unwrap_or_else(|| cells_from_percent(body.height, 70).max(8)),
            },
        }
    }

    /// Build a single-leaf modal layer for this surface.
    ///
    /// `composer_band_height` applies to ComposerBand surfaces (Select / Dock).
    /// Height is computed from feature **content-row** budgets (see
    /// [`crate::navigation::SelectBandBudget`]), not a fixed body percent.
    pub fn modal_layer(
        self,
        body: Rect,
        composer_band_height: u16,
        centered_size: Option<(u16, u16)>,
    ) -> ModalLayer<Region> {
        let placement = self.modal_placement(body, centered_size);
        let host_band_height = match placement {
            ModalPlacement::ComposerBand => composer_band_height,
            _ => 0,
        };
        ModalLayer {
            placement,
            host_band_height,
            tree: leaf(Region::Surface(self)),
        }
    }

    pub fn covers_body(self) -> bool {
        matches!(self.intent(), SurfaceIntent::Browse)
    }
}
