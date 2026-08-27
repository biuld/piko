use std::{env, io::IsTerminal};

/// Piko's terminal-neutral subset of the progressive keyboard protocol.
///
/// This deliberately mirrors only the flags piko can reason about. Conversion
/// to Crossterm's flags is kept at the terminal session boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct KeyboardEnhancements(u8);

impl KeyboardEnhancements {
    pub const DISAMBIGUATE: Self = Self(0b0000_0001);
    pub const EVENT_TYPES: Self = Self(0b0000_0010);
    pub const ALTERNATE_KEYS: Self = Self(0b0000_0100);
    pub const ALL_KEYS: Self = Self(0b0000_1000);
    #[allow(dead_code)]
    pub const ASSOCIATED_TEXT: Self = Self(0b0001_0000);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for KeyboardEnhancements {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for KeyboardEnhancements {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Support level for an optional terminal facility.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

impl Support {
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Supported | Self::Unknown)
    }
}

/// Keyboard protocol features observed during capability detection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardCapabilities {
    pub progressive_protocol: Support,
    pub reported_flags: KeyboardEnhancements,
}

impl KeyboardCapabilities {
    pub const fn baseline() -> Self {
        Self {
            progressive_protocol: Support::Unsupported,
            reported_flags: KeyboardEnhancements::empty(),
        }
    }

    pub const fn enhanced() -> Self {
        Self {
            progressive_protocol: Support::Supported,
            reported_flags: KeyboardEnhancements::DISAMBIGUATE,
        }
    }
}

/// Facts discovered about the current terminal connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalCapabilities {
    pub keyboard: KeyboardCapabilities,
    pub color: ColorLevel,
    pub mouse: Support,
    pub bracketed_paste: Support,
    pub focus_events: Support,
    pub synchronized_output: Support,
}

impl TerminalCapabilities {
    /// A deterministic conservative profile useful for tests and non-TTY
    /// clients.
    #[cfg(test)]
    pub const fn conservative() -> Self {
        Self {
            keyboard: KeyboardCapabilities::baseline(),
            color: ColorLevel::TerminalDefault,
            mouse: Support::Unsupported,
            bracketed_paste: Support::Unsupported,
            focus_events: Support::Unsupported,
            synchronized_output: Support::Unsupported,
        }
    }

    pub const fn enhanced_for_test() -> Self {
        Self {
            keyboard: KeyboardCapabilities::enhanced(),
            color: ColorLevel::TrueColor,
            mouse: Support::Supported,
            bracketed_paste: Support::Supported,
            focus_events: Support::Supported,
            synchronized_output: Support::Supported,
        }
    }
}

/// Effective color depth selected from environment evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorLevel {
    TerminalDefault,
    Ansi16,
    Ansi256,
    TrueColor,
}

impl ColorLevel {
    pub fn from_environment() -> Self {
        let color_term = env::var("COLORTERM")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(color_term.as_str(), "truecolor" | "24bit") {
            return Self::TrueColor;
        }

        let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
        if term.contains("256color") {
            Self::Ansi256
        } else if term.is_empty() {
            Self::TerminalDefault
        } else {
            Self::Ansi16
        }
    }
}

/// A capability detector that can be replaced by deterministic test doubles.
pub trait CapabilityDetector {
    fn detect(&self) -> TerminalCapabilities;
}

/// Production detector. Crossterm owns the bounded progressive-keyboard
/// query; all other optional modes remain unknown until a concrete event path
/// proves otherwise.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCapabilityDetector;

impl CapabilityDetector for SystemCapabilityDetector {
    fn detect(&self) -> TerminalCapabilities {
        let keyboard = match crossterm::terminal::supports_keyboard_enhancement() {
            Ok(true) => KeyboardCapabilities::enhanced(),
            Ok(false) => KeyboardCapabilities::baseline(),
            Err(_) => KeyboardCapabilities {
                progressive_protocol: Support::Unknown,
                reported_flags: KeyboardEnhancements::empty(),
            },
        };
        let tty = std::io::stdout().is_terminal();
        let optional = if tty {
            Support::Unknown
        } else {
            Support::Unsupported
        };
        TerminalCapabilities {
            keyboard,
            color: ColorLevel::from_environment(),
            mouse: optional,
            bracketed_paste: optional,
            focus_events: optional,
            synchronized_output: optional,
        }
    }
}
