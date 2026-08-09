//! Product-layer Action contract for interactive components.

use crate::{
    app::{HitId, command::Action},
    theme::Theme,
};
pub use piko_tui_layout::{ComponentHit, PointerGesture};
use ratatui::{Frame, layout::Rect, style::Style};

/// Business interpretation of pointer hits. Implementations may update state
/// they own and return the same product actions used by keyboard input.
pub trait PointerComponent<E> {
    fn pointer_event(&mut self, _hit: ComponentHit<E>, _gesture: PointerGesture) -> Vec<Action> {
        Vec::new()
    }
}

pub fn paint_element_hover(
    frame: &mut Frame<'_>,
    regions: &[(Rect, HitId)],
    interaction: piko_tui_layout::InteractionState<HitId>,
    excluded: Option<HitId>,
    theme: &Theme,
) {
    let Some(element) = interaction.hovered else {
        return;
    };
    if excluded == Some(element) {
        return;
    }
    let Some(background) = crate::ui::components::hover_bg(theme) else {
        return;
    };
    if let Some((rect, _)) = regions.iter().find(|(_, id)| *id == element) {
        frame
            .buffer_mut()
            .set_style(*rect, Style::default().bg(background));
    }
}
