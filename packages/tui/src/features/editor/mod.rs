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
        if gesture == PointerGesture::Activate && hit.element == Some(HitId::Composer) {
            self.move_to_column(hit.rect.width, hit.local_x());
        }
        Vec::new()
    }
}
