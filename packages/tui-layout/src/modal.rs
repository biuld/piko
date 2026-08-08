//! Modal z-axis layers (product content is opaque to the engine).

use crate::flex::Node;

/// Where a modal layer is rooted inside the body area handed by the shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ModalPlacement {
    /// Entire body rect.
    CoverBody,
    /// Bottom band of body; height from [`ModalLayer::host_band_height`].
    ComposerBand,
    /// Centered inset rect within body.
    Centered { max_width: u16, max_height: u16 },
}

/// One z-axis modal: client-built flex tree + placement.
#[derive(Clone, Debug)]
pub struct ModalLayer<R> {
    pub placement: ModalPlacement,
    /// Used when `placement` is [`ModalPlacement::ComposerBand`].
    pub host_band_height: u16,
    pub tree: Node<R>,
}
