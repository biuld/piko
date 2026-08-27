use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use crate::input::binding::{KeyStroke, Modifiers};

use super::TerminalProfile;

/// Terminal-neutral input after one normalization pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizedInput {
    Key {
        stroke: KeyStroke,
        phase: KeyPhase,
        text: Option<String>,
    },
    Paste(String),
    Pointer(PointerEvent),
    Resize {
        width: u16,
        height: u16,
    },
    FocusGained,
    FocusLost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyPhase {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerEvent {
    pub column: u16,
    pub row: u16,
    pub kind: PointerKind,
    pub modifiers: Modifiers,
}

/// Converts Crossterm's event representation into the TUI input contract.
#[derive(Clone, Debug)]
pub struct InputNormalizer {
    profile: TerminalProfile,
}

impl InputNormalizer {
    pub fn new(profile: TerminalProfile) -> Self {
        Self { profile }
    }

    pub fn normalize(&self, event: Event) -> Option<NormalizedInput> {
        match event {
            Event::Key(key) => self.normalize_key(key),
            Event::Paste(text) => Some(NormalizedInput::Paste(text)),
            Event::Resize(width, height) => Some(NormalizedInput::Resize { width, height }),
            Event::FocusGained => Some(NormalizedInput::FocusGained),
            Event::FocusLost => Some(NormalizedInput::FocusLost),
            Event::Mouse(mouse) => Some(NormalizedInput::Pointer(mouse.into())),
        }
    }

    pub fn normalize_key(&self, key: KeyEvent) -> Option<NormalizedInput> {
        let stroke = KeyStroke::from_event(key)?;
        let phase = match key.kind {
            KeyEventKind::Press => KeyPhase::Press,
            KeyEventKind::Repeat => KeyPhase::Repeat,
            KeyEventKind::Release => KeyPhase::Release,
        };
        // Crossterm's associated-text enhancement is not available in the
        // supported release. Preserve normal character input and never claim
        // that control/meta input produces text.
        let text = if self
            .profile
            .active_keyboard_flags
            .contains(super::KeyboardEnhancements::ALL_KEYS)
        {
            None
        } else {
            match key.code {
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(ch.to_string())
                }
                _ => None,
            }
        };
        Some(NormalizedInput::Key {
            stroke,
            phase,
            text,
        })
    }

    pub fn normalize_paste(&self, text: String) -> NormalizedInput {
        NormalizedInput::Paste(text)
    }
}

impl From<crossterm::event::MouseEvent> for PointerEvent {
    fn from(mouse: crossterm::event::MouseEvent) -> Self {
        Self {
            column: mouse.column,
            row: mouse.row,
            kind: match mouse.kind {
                MouseEventKind::Down(button) => PointerKind::Down(button),
                MouseEventKind::Up(button) => PointerKind::Up(button),
                MouseEventKind::Drag(button) => PointerKind::Drag(button),
                MouseEventKind::Moved => PointerKind::Moved,
                MouseEventKind::ScrollUp => PointerKind::ScrollUp,
                MouseEventKind::ScrollDown => PointerKind::ScrollDown,
                MouseEventKind::ScrollLeft => PointerKind::ScrollLeft,
                MouseEventKind::ScrollRight => PointerKind::ScrollRight,
            },
            modifiers: Modifiers::new(
                mouse.modifiers.contains(KeyModifiers::CONTROL),
                mouse.modifiers.contains(KeyModifiers::ALT),
                mouse.modifiers.contains(KeyModifiers::SHIFT),
            ),
        }
    }
}

impl From<KeyEvent> for NormalizedInput {
    fn from(value: KeyEvent) -> Self {
        // This conversion is primarily a compatibility/testing adapter. The
        // production event loop owns a profile-aware normalizer.
        InputNormalizer::new(TerminalProfile::enhanced_for_test())
            .normalize_key(value)
            .unwrap_or(NormalizedInput::FocusLost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::binding::Key;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn normalizes_printable_text_without_claiming_control_text() {
        let normalizer = InputNormalizer::new(TerminalProfile::baseline());
        let NormalizedInput::Key {
            stroke,
            text,
            phase,
        } = normalizer
            .normalize_key(key(
                KeyCode::Char('你'),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            ))
            .unwrap()
        else {
            panic!("expected key input");
        };
        assert_eq!(stroke.key, Key::Char('你'));
        assert_eq!(text.as_deref(), Some("你"));
        assert_eq!(phase, KeyPhase::Press);

        let NormalizedInput::Key { text, .. } = normalizer
            .normalize_key(key(
                KeyCode::Char('j'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ))
            .unwrap()
        else {
            panic!("expected control key input");
        };
        assert_eq!(text, None);
    }

    #[test]
    fn preserves_key_phase_and_modified_enter_identity() {
        let normalizer = InputNormalizer::new(TerminalProfile::enhanced_for_test());
        let NormalizedInput::Key {
            stroke,
            phase,
            text,
        } = normalizer
            .normalize_key(key(
                KeyCode::Enter,
                KeyModifiers::SHIFT,
                KeyEventKind::Repeat,
            ))
            .unwrap()
        else {
            panic!("expected key input");
        };
        assert_eq!(stroke, KeyStroke::parse("shift+enter").unwrap());
        assert_eq!(phase, KeyPhase::Repeat);
        assert_eq!(text, None);

        let NormalizedInput::Key { phase, .. } = normalizer
            .normalize_key(key(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ))
            .unwrap()
        else {
            panic!("expected key input");
        };
        assert_eq!(phase, KeyPhase::Release);
    }

    #[test]
    fn normalizes_optional_events_without_terminal_specific_branches() {
        let normalizer = InputNormalizer::new(TerminalProfile::baseline());
        assert_eq!(
            normalizer.normalize(Event::Paste("hello".to_string())),
            Some(NormalizedInput::Paste("hello".to_string()))
        );
        assert_eq!(
            normalizer.normalize(Event::Resize(80, 24)),
            Some(NormalizedInput::Resize {
                width: 80,
                height: 24
            })
        );
    }

    #[test]
    fn normalizes_pointer_and_focus_events_to_semantic_values() {
        let normalizer = InputNormalizer::new(TerminalProfile::baseline());
        let mouse = crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 2,
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        };

        assert_eq!(
            normalizer.normalize(Event::Mouse(mouse)),
            Some(NormalizedInput::Pointer(PointerEvent {
                column: 4,
                row: 2,
                kind: PointerKind::Down(MouseButton::Left),
                modifiers: Modifiers::new(true, false, true),
            }))
        );
        assert_eq!(
            normalizer.normalize(Event::FocusGained),
            Some(NormalizedInput::FocusGained)
        );
        assert_eq!(
            normalizer.normalize(Event::FocusLost),
            Some(NormalizedInput::FocusLost)
        );
    }
}
