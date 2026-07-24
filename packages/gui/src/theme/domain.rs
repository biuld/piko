//! Product domain role colors (chat authors, tool classes, …).
//!
//! These are **not** part of chrome-core tokens. A multi-pane client that only
//! depends on `island` need not link this module.

use gpui::{Hsla, Rgba};
use island::theme::{IslandPalette, IslandTokens, island_palette};

/// Domain-specific accent roles used by timeline / tree / conversation chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainRole {
    User,
    Assistant,
    Thinking,
    Tool,
    System,
}

fn domain_role_hex_for(palette: IslandPalette, role: DomainRole) -> u32 {
    match (palette, role) {
        (IslandPalette::Dark, DomainRole::User) => 0x87c3ff,
        (IslandPalette::Dark, DomainRole::Assistant) => 0x82d2ce,
        (IslandPalette::Dark, DomainRole::Thinking) => 0x909194,
        (IslandPalette::Dark, DomainRole::Tool) => 0xebc88d,
        (IslandPalette::Dark, DomainRole::System) => 0x6e747b,
        (IslandPalette::Light, DomainRole::User) => 0x1749bd,
        (IslandPalette::Light, DomainRole::Assistant) => 0x14646e,
        (IslandPalette::Light, DomainRole::Thinking) => 0x747576,
        (IslandPalette::Light, DomainRole::Tool) => 0x5511bf,
        (IslandPalette::Light, DomainRole::System) => 0x6e747b,
    }
}

fn domain_role_hex(role: DomainRole) -> u32 {
    domain_role_hex_for(island_palette(), role)
}

pub fn domain_role_rgba(role: DomainRole) -> Rgba {
    IslandTokens::rgba(domain_role_hex(role))
}

pub fn domain_role_hsla(role: DomainRole) -> Hsla {
    IslandTokens::hsla(domain_role_hex(role))
}

#[cfg(test)]
mod tests {
    use island::theme::IslandPalette;

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
        for palette in [IslandPalette::Dark, IslandPalette::Light] {
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
            domain_role_hex_for(IslandPalette::Light, DomainRole::User),
            0x1749bd
        );
        assert_eq!(
            domain_role_hex_for(IslandPalette::Light, DomainRole::Assistant),
            0x14646e
        );
        assert_eq!(
            domain_role_hex_for(IslandPalette::Light, DomainRole::Thinking),
            0x747576
        );
        assert_eq!(
            domain_role_hex_for(IslandPalette::Light, DomainRole::Tool),
            0x5511bf
        );
    }
}
