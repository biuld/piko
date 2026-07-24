//! Settings IA — section identity and i18n keys.

/// Active section within the Settings Archipelago.
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

    /// Move selection by `delta` positions (wraps). Test/helper only —
    /// production nav cursor uses `piko_chrome::components::list::ListKeyboard`.
    #[cfg(test)]
    pub fn offset(self, delta: isize) -> Self {
        let all = Self::ALL;
        let ix = all.iter().position(|s| *s == self).unwrap_or(0) as isize;
        let n = all.len() as isize;
        let next = (ix + delta).rem_euclid(n) as usize;
        all[next]
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

    #[test]
    fn section_offset_wraps() {
        assert_eq!(
            SettingsSection::General.offset(-1),
            SettingsSection::Advanced
        );
        assert_eq!(
            SettingsSection::Advanced.offset(1),
            SettingsSection::General
        );
        assert_eq!(SettingsSection::General.offset(1), SettingsSection::Account);
    }
}
