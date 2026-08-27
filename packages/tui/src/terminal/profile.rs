use super::text::TerminalTextPolicy;
use super::{ColorLevel, KeyboardEnhancements, TerminalCapabilities, TerminalModePlan};

/// Reachability facts used by keybinding resolution and guidance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyReachability {
    pub enhanced_keyboard: bool,
}

impl KeyReachability {
    pub const fn baseline() -> Self {
        Self {
            enhanced_keyboard: false,
        }
    }

    pub const fn enhanced() -> Self {
        Self {
            enhanced_keyboard: true,
        }
    }

    pub const fn shift_enter(self) -> bool {
        self.enhanced_keyboard
    }
}

/// Immutable behavior selected for one TUI process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalProfile {
    pub capabilities: TerminalCapabilities,
    pub modes: TerminalModePlan,
    pub color: ColorLevel,
    pub active_keyboard_flags: KeyboardEnhancements,
    pub key_reachability: KeyReachability,
    pub text: TerminalTextPolicy,
}

impl TerminalProfile {
    pub fn resolve(capabilities: TerminalCapabilities) -> Self {
        let modes = TerminalModePlan::from_capabilities(&capabilities);
        let enhanced = modes
            .keyboard_flags
            .contains(KeyboardEnhancements::DISAMBIGUATE);
        Self {
            color: capabilities.color,
            active_keyboard_flags: modes.keyboard_flags,
            key_reachability: if enhanced {
                KeyReachability::enhanced()
            } else {
                KeyReachability::baseline()
            },
            text: TerminalTextPolicy,
            capabilities,
            modes,
        }
    }

    #[cfg(test)]
    pub fn baseline() -> Self {
        Self::resolve(TerminalCapabilities::conservative())
    }

    pub fn enhanced_for_test() -> Self {
        Self::resolve(TerminalCapabilities::enhanced_for_test())
    }
}
