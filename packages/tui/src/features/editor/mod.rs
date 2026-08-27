pub mod state;

pub use state::Editor;

use crate::{
    app::HitId,
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

impl PointerComponent<HitId> for Editor {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        if hit.element != Some(HitId::Composer) {
            return Vec::new();
        }

        let visible_rows = hit.rect.height.saturating_sub(2).max(1);
        match gesture {
            PointerGesture::ScrollUp => self.scroll_up(
                hit.rect.width,
                visible_rows,
                crate::features::timeline::WHEEL_STEP,
            ),
            PointerGesture::ScrollDown => self.scroll_down(
                hit.rect.width,
                visible_rows,
                crate::features::timeline::WHEEL_STEP,
            ),
            PointerGesture::Activate => {
                let local_x = hit.local_x();
                let local_y = hit.local_y();
                let gutter_x = hit.rect.width.saturating_sub(1);
                let is_scrollbar_gutter = local_x >= gutter_x
                    && local_y > 0
                    && local_y < hit.rect.height.saturating_sub(1);
                if is_scrollbar_gutter {
                    self.scroll_to_row(hit.rect.width, visible_rows, local_y.saturating_sub(1));
                } else {
                    self.move_to_position(hit.rect.width, hit.rect.height, local_x, local_y);
                }
            }
        }
        Vec::new()
    }
}
