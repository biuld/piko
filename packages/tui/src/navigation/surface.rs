//! Product surface catalog and modal placement policy.
//!
//! Intent split (z-stack only — plane is never rebuilt for a surface):
//! - **Browse** → [`ModalPlacement::CoverBody`]
//! - **Select** → [`ModalPlacement::ComposerBand`]
//! - **Dock** → [`ModalPlacement::ComposerBand`] (approval and tool
//!   interaction replace the composer)
//! - **Modal** → [`ModalPlacement::Centered`] (settings dialog)

use piko_tui_layout::{
    FlexItem, ModalLayer, ModalPlacement, cells_from_percent, flex_column, leaf,
};
use ratatui::layout::Rect;

use crate::features::dock_stack::GUIDANCE_HEIGHT;

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
    Usage,
    Notifications,
    Mcp,
    Processes,
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
    /// Centered dialog.
    Modal,
}

/// Pointer policy when a click resolves below the active modal layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutsideClickPolicy {
    /// Close or step back exactly like the surface's keyboard cancel action.
    Dismiss,
    /// Consume the click without closing or reaching a lower layer.
    Block,
}

impl SurfaceId {
    pub fn intent(self) -> SurfaceIntent {
        match self {
            Self::Sessions | Self::Tree | Self::Diagnostics | Self::SummaryPrompt => {
                SurfaceIntent::Browse
            }

            // Session agent picker (viewed-agent switch) sits near the composer,
            // same as Models / Thinking / Auth.
            Self::Agents
            | Self::Models
            | Self::Thinking
            | Self::AuthSelector
            | Self::Mcp
            | Self::Processes => SurfaceIntent::Select,

            Self::Approval | Self::ToolInteraction => SurfaceIntent::Dock,

            Self::Settings | Self::Usage | Self::Notifications => SurfaceIntent::Modal,
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
    /// `composer_band_height` is the Select / Dock surface height. Its modal
    /// host also prepends the resident Guidance row. Surface height is computed
    /// from feature **content-row** budgets (see
    /// [`crate::navigation::SelectBandBudget`]), not a fixed body percent.
    pub fn modal_layer(
        self,
        body: Rect,
        composer_band_height: u16,
        centered_size: Option<(u16, u16)>,
    ) -> ModalLayer<Region> {
        let placement = self.modal_placement(body, centered_size);
        let host_band_height = match placement {
            ModalPlacement::ComposerBand => composer_band_height.saturating_add(GUIDANCE_HEIGHT),
            _ => 0,
        };
        let tree = match placement {
            ModalPlacement::ComposerBand => flex_column(vec![
                FlexItem::fixed(GUIDANCE_HEIGHT, leaf(Region::Guidance)),
                FlexItem::fixed(composer_band_height, leaf(Region::Surface(self))),
            ]),
            _ => leaf(Region::Surface(self)),
        };
        ModalLayer {
            placement,
            host_band_height,
            tree,
        }
    }

    pub fn covers_body(self) -> bool {
        matches!(self.intent(), SurfaceIntent::Browse)
    }

    pub fn outside_click_policy(self) -> OutsideClickPolicy {
        match self.intent() {
            SurfaceIntent::Dock => OutsideClickPolicy::Block,
            SurfaceIntent::Browse | SurfaceIntent::Select | SurfaceIntent::Modal => {
                OutsideClickPolicy::Dismiss
            }
        }
    }
}
