use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Logical key names supported by the normalized terminal contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
}

impl Key {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        Some(match lower.as_str() {
            "enter" | "return" => Self::Enter,
            "esc" | "escape" => Self::Escape,
            "backspace" | "backspacekey" => Self::Backspace,
            "delete" | "del" => Self::Delete,
            "tab" | "backtab" => Self::Tab,
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" | "pgup" => Self::PageUp,
            "pagedown" | "pgdn" => Self::PageDown,
            value if value.len() > 1 && value.starts_with('f') => {
                Self::Function(value[1..].parse().ok()?)
            }
            value => {
                let mut chars = value.chars();
                let ch = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                Self::Char(ch)
            }
        })
    }

    pub const fn is_shift_enter(self, modifiers: Modifiers) -> bool {
        matches!(self, Self::Enter) && modifiers.shift && !modifiers.ctrl && !modifiers.alt
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };

    pub const fn new(ctrl: bool, alt: bool, shift: bool) -> Self {
        Self { ctrl, alt, shift }
    }
}

/// Canonical key plus normalized modifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeyStroke {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyStroke {
    pub fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::NONE;
        let mut key = None;
        for part in value.split('+') {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => {
                    if modifiers.ctrl {
                        return None;
                    }
                    modifiers.ctrl = true;
                }
                "alt" | "option" => {
                    if modifiers.alt {
                        return None;
                    }
                    modifiers.alt = true;
                }
                "shift" => {
                    if modifiers.shift {
                        return None;
                    }
                    modifiers.shift = true;
                }
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(Key::parse(part)?);
                }
            }
        }
        Some(Self {
            key: key?,
            modifiers,
        })
    }

    pub fn from_event(event: KeyEvent) -> Option<Self> {
        let key = match event.code {
            KeyCode::Char(ch) => Key::Char(ch.to_ascii_lowercase()),
            KeyCode::Enter => Key::Enter,
            KeyCode::Esc => Key::Escape,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Delete => Key::Delete,
            KeyCode::Tab | KeyCode::BackTab => Key::Tab,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::F(number) => Key::Function(number),
            _ => return None,
        };
        let mut modifiers = Modifiers::new(
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
            event.modifiers.contains(KeyModifiers::SHIFT),
        );
        // BackTab is Crossterm's semantic spelling of Shift+Tab; some
        // backends omit the SHIFT modifier on the event itself.
        if event.code == KeyCode::BackTab {
            modifiers.shift = true;
        }
        Some(Self { key, modifiers })
    }

    /// Compact label used by user-facing guidance.  Configuration and
    /// diagnostics continue to use the canonical `Display` spelling.
    pub fn hint(self) -> String {
        let mut label = String::new();
        if self.modifiers.ctrl {
            label.push_str("Ctrl+");
        }
        if self.modifiers.alt {
            label.push_str("Alt+");
        }
        if self.modifiers.shift {
            label.push_str("Shift+");
        }
        label.push_str(match self.key {
            Key::Char(ch) => return format!("{label}{}", ch.to_ascii_uppercase()),
            Key::Enter => "Enter",
            Key::Escape => "Esc",
            Key::Backspace => "Backspace",
            Key::Delete => "Delete",
            Key::Tab => "Tab",
            Key::Left => "←",
            Key::Right => "→",
            Key::Up => "↑",
            Key::Down => "↓",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "PgUp",
            Key::PageDown => "PgDn",
            Key::Function(number) => return format!("{label}F{number}"),
        });
        label
    }
}

impl fmt::Display for KeyStroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.modifiers.alt {
            f.write_str("Alt+")?;
        }
        if self.modifiers.shift {
            f.write_str("Shift+")?;
        }
        match self.key {
            Key::Char(ch) => write!(f, "{}", ch.to_ascii_uppercase()),
            Key::Enter => f.write_str("Enter"),
            Key::Escape => f.write_str("Esc"),
            Key::Backspace => f.write_str("Backspace"),
            Key::Delete => f.write_str("Delete"),
            Key::Tab => f.write_str("Tab"),
            Key::Left => f.write_str("Left"),
            Key::Right => f.write_str("Right"),
            Key::Up => f.write_str("Up"),
            Key::Down => f.write_str("Down"),
            Key::Home => f.write_str("Home"),
            Key::End => f.write_str("End"),
            Key::PageUp => f.write_str("PageUp"),
            Key::PageDown => f.write_str("PageDown"),
            Key::Function(number) => write!(f, "F{number}"),
        }
    }
}
