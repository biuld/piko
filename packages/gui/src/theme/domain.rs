//! Product domain role colors (chat authors, tool classes, …).
//!
//! These are **not** part of chrome-core tokens. A multi-pane client that only
//! depends on `piko-chrome` need not link this module.

use gpui::{Hsla, Rgba};
use piko_chrome::{ChromePalette, ChromeTokens, chrome_palette};

/// Domain-specific accent roles used by timeline / tree / conversation chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainRole {
    User,
    Assistant,
    Thinking,
    Tool,
    System,
}

fn domain_role_hex_for(palette: ChromePalette, role: DomainRole) -> u32 {
    match (palette, role) {
        (ChromePalette::Dark, DomainRole::User) => 0x87c3ff,
        (ChromePalette::Dark, DomainRole::Assistant) => 0x82d2ce,
        (ChromePalette::Dark, DomainRole::Thinking) => 0x909194,
        (ChromePalette::Dark, DomainRole::Tool) => 0xebc88d,
        (ChromePalette::Dark, DomainRole::System) => 0x6e747b,
        (ChromePalette::Light, DomainRole::User) => 0x1749bd,
        (ChromePalette::Light, DomainRole::Assistant) => 0x14646e,
        (ChromePalette::Light, DomainRole::Thinking) => 0x747576,
        (ChromePalette::Light, DomainRole::Tool) => 0x5511bf,
        (ChromePalette::Light, DomainRole::System) => 0x6e747b,
    }
}

fn domain_role_hex(role: DomainRole) -> u32 {
    domain_role_hex_for(chrome_palette(), role)
}

pub fn domain_role_rgba(role: DomainRole) -> Rgba {
    ChromeTokens::rgba(domain_role_hex(role))
}

pub fn domain_role_hsla(role: DomainRole) -> Hsla {
    ChromeTokens::hsla(domain_role_hex(role))
}

#[cfg(test)]
mod tests {
    use piko_chrome::ChromePalette;

    use super::{DomainRole, domain_role_hex_for, domain_role_rgba};

    #[test]
    fn domain_roles_are_distinct_from_each_other() {
        let roles = [
            DomainRole::User,
            DomainRole::Assistant,
            DomainRole::Thinking,
            DomainRole::Tool,
            DomainRole::System,
        ];
        for palette in [ChromePalette::Dark, ChromePalette::Light] {
            for (i, a) in roles.iter().enumerate() {
                for b in roles.iter().skip(i + 1) {
                    assert_ne!(
                        domain_role_hex_for(palette, *a),
                        domain_role_hex_for(palette, *b)
                    );
                }
            }
        }
        // Smoke: resolves to a real color value.
        let _ = domain_role_rgba(DomainRole::User);
    }

    #[test]
    fn fleet_light_roles_are_stable() {
        assert_eq!(
            domain_role_hex_for(ChromePalette::Light, DomainRole::User),
            0x1749bd
        );
        assert_eq!(
            domain_role_hex_for(ChromePalette::Light, DomainRole::Assistant),
            0x14646e
        );
        assert_eq!(
            domain_role_hex_for(ChromePalette::Light, DomainRole::Thinking),
            0x747576
        );
        assert_eq!(
            domain_role_hex_for(ChromePalette::Light, DomainRole::Tool),
            0x5511bf
        );
    }
}
