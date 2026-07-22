//! Responsive overlay panel geometry from preferred size + viewport.

use gpui::{Pixels, Size, px};

use super::surface::OverlayPanelStyle;

/// Resolved panel box inside the viewport (chrome-owned margins).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayEnvelope {
    pub width: Pixels,
    pub max_height: Pixels,
    pub top_pad: Pixels,
    pub bottom_pad: Pixels,
    pub h_margin: Pixels,
}

const H_MARGIN: f32 = 24.;
const V_MARGIN: f32 = 24.;
const MIN_WIDTH: f32 = 280.;
const MIN_HEIGHT: f32 = 160.;

/// Compute a panel envelope that fits inside `viewport` when provided.
///
/// Without a viewport, falls back to a large desktop assumption so callers that
/// have not yet threaded bounds still get stable defaults.
pub fn overlay_envelope(
    preferred_width: Pixels,
    style: OverlayPanelStyle,
    viewport: Option<Size<Pixels>>,
) -> OverlayEnvelope {
    let vw: f32 = viewport.map(|v| f32::from(v.width)).unwrap_or(1360.);
    let vh: f32 = viewport.map(|v| f32::from(v.height)).unwrap_or(840.);

    let h_margin = H_MARGIN;
    let max_w = (vw - 2. * h_margin).max(MIN_WIDTH);
    let pref: f32 = preferred_width.into();
    let width = pref.min(max_w).max(MIN_WIDTH.min(max_w));

    let (top_frac, top_min, top_max, abs_max_h) = match style {
        OverlayPanelStyle::Dialog => (0.12_f32, 48., 96., 640.),
        OverlayPanelStyle::Palette => (0.09_f32, 40., 72., 420.),
    };
    let top_pad = (vh * top_frac).clamp(top_min, top_max);
    let bottom_pad = V_MARGIN;
    let available = (vh - top_pad - bottom_pad).max(MIN_HEIGHT);
    let max_height = available.min(abs_max_h).max(MIN_HEIGHT.min(available));

    OverlayEnvelope {
        width: px(width),
        max_height: px(max_height),
        top_pad: px(top_pad),
        bottom_pad: px(bottom_pad),
        h_margin: px(h_margin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn clamps_width_on_narrow_viewport() {
        let env = overlay_envelope(
            px(560.),
            OverlayPanelStyle::Dialog,
            Some(size(px(400.), px(800.))),
        );
        // 400 - 48 margins = 352
        assert!(f32::from(env.width) <= 352. + 0.5);
        assert!(f32::from(env.width) >= MIN_WIDTH.min(352.) - 0.5);
    }

    #[test]
    fn clamps_height_on_short_viewport() {
        let env = overlay_envelope(
            px(420.),
            OverlayPanelStyle::Dialog,
            Some(size(px(800.), px(400.))),
        );
        assert!(f32::from(env.max_height) < 640.);
        assert!(f32::from(env.max_height) >= MIN_HEIGHT - 0.5);
    }

    #[test]
    fn prefers_width_when_viewport_is_wide() {
        let env = overlay_envelope(
            px(560.),
            OverlayPanelStyle::Dialog,
            Some(size(px(1400.), px(900.))),
        );
        assert!((f32::from(env.width) - 560.).abs() < 0.5);
    }
}
