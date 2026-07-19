//! Settings IA — section identity and i18n keys.

/// Active section within the Settings Primary Surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    General,
    Account,
    AgentTools,
    ContextReliability,
    Appearance,
    Keyboard,
    Advanced,
}

impl SettingsSection {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Account,
        Self::AgentTools,
        Self::ContextReliability,
        Self::Appearance,
        Self::Keyboard,
        Self::Advanced,
    ];

    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::General => "settings.section.general",
            Self::Account => "settings.section.account",
            Self::AgentTools => "settings.section.agent_tools",
            Self::ContextReliability => "settings.section.context_reliability",
            Self::Appearance => "settings.section.appearance",
            Self::Keyboard => "settings.section.keyboard",
            Self::Advanced => "settings.section.advanced",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sections_have_unique_keys() {
        let mut keys = std::collections::HashSet::new();
        for section in SettingsSection::ALL {
            assert!(keys.insert(section.i18n_key()));
        }
    }
}
