//! Primary Surface navigation state — full-frame Workbench vs Settings.

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

/// Which full chrome frame (TitleBar + body + optional StatusBar) is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimarySurface {
    #[default]
    Workbench,
    Settings {
        section: SettingsSection,
    },
}

impl PrimarySurface {
    pub fn is_workbench(self) -> bool {
        matches!(self, Self::Workbench)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_workbench() {
        assert!(PrimarySurface::default().is_workbench());
    }

    #[test]
    fn settings_carries_section() {
        let surface = PrimarySurface::Settings {
            section: SettingsSection::Appearance,
        };
        assert!(matches!(
            surface,
            PrimarySurface::Settings {
                section: SettingsSection::Appearance
            }
        ));
    }

    #[test]
    fn all_sections_have_unique_keys() {
        let mut keys = std::collections::HashSet::new();
        for section in SettingsSection::ALL {
            assert!(keys.insert(section.i18n_key()));
        }
    }
}
