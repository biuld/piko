//! HostPrompt presentation slot owned by the overlay shell.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Approval,
    Interaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFront {
    pub kind: PromptKind,
    pub id: String,
    pub agent_instance_id: String,
    pub remaining: usize,
    pub response_in_flight: bool,
    pub summary: String,
}

pub fn prompt_fingerprint(front: Option<&PromptFront>) -> Option<String> {
    front.map(|f| format!("{:?}:{}", f.kind, f.id))
}
