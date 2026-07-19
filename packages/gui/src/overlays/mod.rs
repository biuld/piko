//! PromptCoordinator projections and HostPrompt bodies.

mod approval_view;
mod interaction_view;
mod prompt_coordinator;

pub use approval_view::{approval_title, render_approval_body};
pub use interaction_view::{InteractionForm, interaction_title};
pub use prompt_coordinator::{PromptFront, PromptKind, derive_prompt_front, prompt_fingerprint};
