//! Primary Surface navigation state — full-frame Workbench vs Settings.
//!
//! Owned by the composition root so shell never depends on feature IA types.

use crate::features::SettingsSection;

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
}
