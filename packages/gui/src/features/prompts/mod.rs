//! Approval and interaction HostPrompt bodies + prompt front projection.

mod approval;
mod coordinator;
mod interaction;

pub use approval::{approval_title, render_approval_body};
pub use coordinator::{PromptFront, PromptKind, derive_prompt_front};
pub use interaction::{InteractionForm, interaction_title};
