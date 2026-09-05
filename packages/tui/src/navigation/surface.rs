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
    Todos,
    Usage,
    Notifications,
    ThoughtInspector,
    Mcp,
    Processes,
    Diagnostics,
    History,
    Approval,
    ToolInteraction,
    SummaryPrompt,
    AuthSelector,
}

/// Pointer policy when a click resolves below the active modal layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutsideClickPolicy {
    /// Close or step back exactly like the surface's keyboard cancel action.
    Dismiss,
    /// Consume the click without closing or reaching a lower layer.
    Block,
}

/// Product-facing mount intent derived from sizing and modal barrier policy.
/// It is not stored separately in [`SurfaceSpec`], so it cannot disagree with
/// the concrete placement contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceIntent {
    Browse,
    Select,
    Dock,
    Modal,
}

/// Static sizing contract; feature state only supplies the dimensions inside
/// the declared placement class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceSizing {
    CoverBody,
    ComposerBand,
    Centered(CenteredSizePolicy),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CenteredSizePolicy {
    SettingsViewport,
    UsageContent,
    TodoContent,
    NotificationContent,
    ThoughtContent,
}

/// Keyboard routing family. Surface-specific commands remain with the feature,
/// while the router no longer rediscovers which interaction family it belongs
/// to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceInputProfile {
    FilteredSelection,
    NotificationList,
    ReadOnlyViewport,
    ApprovalWorkflow,
    ToolWorkflow,
    SummaryWorkflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SurfaceGuidance {
    None,
    DefaultList,
    Feature,
    Workflow,
}

/// One catalog row consumed by focus, layout, pointer, and guidance policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SurfaceSpec {
    pub sizing: SurfaceSizing,
    pub input: SurfaceInputProfile,
    pub guidance: SurfaceGuidance,
    pub outside_click: OutsideClickPolicy,
}

impl SurfaceId {
    pub const fn spec(self) -> SurfaceSpec {
        use CenteredSizePolicy as Centered;
        use OutsideClickPolicy as Outside;
        use SurfaceGuidance as Guidance;
        use SurfaceInputProfile as Input;
        use SurfaceSizing as Sizing;

        match self {
            Self::Sessions | Self::Tree => SurfaceSpec {
                sizing: Sizing::CoverBody,
                input: Input::FilteredSelection,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::Diagnostics | Self::History => SurfaceSpec {
                sizing: Sizing::CoverBody,
                input: Input::ReadOnlyViewport,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::SummaryPrompt => SurfaceSpec {
                sizing: Sizing::CoverBody,
                input: Input::SummaryWorkflow,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::Agents | Self::Models | Self::Thinking => SurfaceSpec {
                sizing: Sizing::ComposerBand,
                input: Input::FilteredSelection,
                guidance: Guidance::DefaultList,
                outside_click: Outside::Dismiss,
            },
            Self::AuthSelector => SurfaceSpec {
                sizing: Sizing::ComposerBand,
                input: Input::FilteredSelection,
                guidance: Guidance::Feature,
                outside_click: Outside::Dismiss,
            },
            Self::Mcp | Self::Processes => SurfaceSpec {
                sizing: Sizing::ComposerBand,
                input: Input::ReadOnlyViewport,
                guidance: Guidance::Feature,
                outside_click: Outside::Dismiss,
            },
            Self::Approval => SurfaceSpec {
                sizing: Sizing::ComposerBand,
                input: Input::ApprovalWorkflow,
                guidance: Guidance::Workflow,
                outside_click: Outside::Block,
            },
            Self::ToolInteraction => SurfaceSpec {
                sizing: Sizing::ComposerBand,
                input: Input::ToolWorkflow,
                guidance: Guidance::Workflow,
                outside_click: Outside::Block,
            },
            Self::Settings => SurfaceSpec {
                sizing: Sizing::Centered(Centered::SettingsViewport),
                input: Input::FilteredSelection,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::Usage => SurfaceSpec {
                sizing: Sizing::Centered(Centered::UsageContent),
                input: Input::ReadOnlyViewport,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::Todos => SurfaceSpec {
                sizing: Sizing::Centered(Centered::TodoContent),
                input: Input::ReadOnlyViewport,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::Notifications => SurfaceSpec {
                sizing: Sizing::Centered(Centered::NotificationContent),
                input: Input::NotificationList,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
            Self::ThoughtInspector => SurfaceSpec {
                sizing: Sizing::Centered(Centered::ThoughtContent),
                input: Input::ReadOnlyViewport,
                guidance: Guidance::None,
                outside_click: Outside::Dismiss,
            },
        }
    }

    pub fn modal_placement(self, body: Rect, centered_size: Option<(u16, u16)>) -> ModalPlacement {
        match self.spec().sizing {
            SurfaceSizing::CoverBody => ModalPlacement::CoverBody,
            SurfaceSizing::ComposerBand => ModalPlacement::ComposerBand,
            SurfaceSizing::Centered(_) => ModalPlacement::Centered {
                max_width: centered_size
                    .map(|(w, _)| w)
                    .unwrap_or_else(|| cells_from_percent(body.width, 88).max(40)),
                max_height: centered_size
                    .map(|(_, h)| h)
                    .unwrap_or_else(|| cells_from_percent(body.height, 70).max(8)),
            },
        }
    }

    pub fn intent(self) -> SurfaceIntent {
        match (self.spec().sizing, self.spec().outside_click) {
            (SurfaceSizing::CoverBody, _) => SurfaceIntent::Browse,
            (SurfaceSizing::Centered(_), _) => SurfaceIntent::Modal,
            (SurfaceSizing::ComposerBand, OutsideClickPolicy::Block) => SurfaceIntent::Dock,
            (SurfaceSizing::ComposerBand, OutsideClickPolicy::Dismiss) => SurfaceIntent::Select,
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
        matches!(self.spec().sizing, SurfaceSizing::CoverBody)
    }

    pub fn outside_click_policy(self) -> OutsideClickPolicy {
        self.spec().outside_click
    }
}
