//! PromptCoordinator and approval / interaction dialogs.

mod approval_dialog;
mod interaction_dialog;
mod prompt_coordinator;

pub use approval_dialog::open_approval_dialog;
pub use interaction_dialog::open_interaction_dialog;
pub use prompt_coordinator::{PromptFront, PromptKind, derive_prompt_front, prompt_fingerprint};
