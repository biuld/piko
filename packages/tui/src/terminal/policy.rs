use super::{KeyboardEnhancements, Support, TerminalCapabilities};

/// Terminal modes requested by the TUI after capability resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalModePlan {
    pub keyboard_flags: KeyboardEnhancements,
    pub mouse_capture: bool,
    pub bracketed_paste: bool,
    pub focus_events: bool,
    pub synchronized_output: bool,
    pub alternate_screen: bool,
}

impl TerminalModePlan {
    pub fn from_capabilities(capabilities: &TerminalCapabilities) -> Self {
        let keyboard_flags = if capabilities.keyboard.progressive_protocol == Support::Supported {
            KeyboardEnhancements::DISAMBIGUATE
        } else {
            KeyboardEnhancements::empty()
        };
        Self {
            keyboard_flags,
            // These modes are enhancements, not requirements. Unknown means
            // "try and retain the keyboard-only workflow if it is rejected".
            mouse_capture: capabilities.mouse.is_available(),
            bracketed_paste: capabilities.bracketed_paste.is_available(),
            focus_events: false,
            synchronized_output: false,
            alternate_screen: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_capabilities_keep_the_keyboard_only_policy() {
        let plan = TerminalModePlan::from_capabilities(&TerminalCapabilities::conservative());

        assert!(plan.keyboard_flags.is_empty());
        assert!(!plan.mouse_capture);
        assert!(!plan.bracketed_paste);
        assert!(!plan.focus_events);
        assert!(!plan.synchronized_output);
        assert!(plan.alternate_screen);
    }

    #[test]
    fn enhanced_capabilities_request_disambiguation_without_all_keys() {
        let plan = TerminalModePlan::from_capabilities(&TerminalCapabilities::enhanced_for_test());

        assert!(
            plan.keyboard_flags
                .contains(KeyboardEnhancements::DISAMBIGUATE)
        );
        assert!(!plan.keyboard_flags.contains(KeyboardEnhancements::ALL_KEYS));
        assert!(plan.mouse_capture);
        assert!(plan.bracketed_paste);
        assert!(!plan.focus_events);
        assert!(!plan.synchronized_output);
        assert!(plan.alternate_screen);
    }

    #[test]
    fn unknown_optional_support_is_attempted_but_not_required() {
        let capabilities = TerminalCapabilities {
            keyboard: super::super::capability::KeyboardCapabilities {
                progressive_protocol: Support::Unknown,
                reported_flags: KeyboardEnhancements::empty(),
            },
            color: super::super::ColorLevel::TerminalDefault,
            mouse: Support::Unknown,
            bracketed_paste: Support::Unknown,
            focus_events: Support::Unknown,
            synchronized_output: Support::Unknown,
        };
        let plan = TerminalModePlan::from_capabilities(&capabilities);

        assert!(plan.keyboard_flags.is_empty());
        assert!(plan.mouse_capture);
        assert!(plan.bracketed_paste);
        assert!(!plan.focus_events);
        assert!(!plan.synchronized_output);
    }
}
