//! Product domain role colors (chat authors, tool classes, …).
//!
//! These are **not** part of chrome-core tokens. A multi-pane client that only
//! depends on `piko-chrome` need not link this module.

use gpui::{Hsla, Rgba};
use piko_chrome::ChromeTokens;

/// Domain-specific accent roles used by timeline / tree / conversation chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainRole {
    User,
    Assistant,
    Thinking,
    Tool,
    System,
}

fn domain_role_hex(role: DomainRole) -> u32 {
    match role {
        DomainRole::User => 0x87c3ff,
        DomainRole::Assistant => 0x82d2ce,
        DomainRole::Thinking => 0x909194,
        DomainRole::Tool => 0xebc88d,
        DomainRole::System => 0x6e747b,
    }
}

pub fn domain_role_rgba(role: DomainRole) -> Rgba {
    ChromeTokens::rgba(domain_role_hex(role))
}

pub fn domain_role_hsla(role: DomainRole) -> Hsla {
    ChromeTokens::hsla(domain_role_hex(role))
}

#[cfg(test)]
mod tests {
    use super::{DomainRole, domain_role_hex, domain_role_rgba};

    #[test]
    fn domain_roles_are_distinct_from_each_other() {
        let roles = [
            DomainRole::User,
            DomainRole::Assistant,
            DomainRole::Thinking,
            DomainRole::Tool,
            DomainRole::System,
        ];
        for (i, a) in roles.iter().enumerate() {
            for b in roles.iter().skip(i + 1) {
                assert_ne!(domain_role_hex(*a), domain_role_hex(*b));
            }
        }
        // Smoke: resolves to a real color value.
        let _ = domain_role_rgba(DomainRole::User);
    }
}
