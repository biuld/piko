use std::{collections::HashMap, fs, path::Path};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyAction {
    Exit,
    NewLine,
    Sessions,
    SessionTree,
    Commands,
    Settings,
    Status,
    ApprovalAccept,
    ApprovalAcceptSession,
    ApprovalAcceptWorkspace,
    ApprovalDecline,
    ClearNotifications,
    HistoryPrev,
    HistoryNext,
    SelectPrev,
    SelectNext,
    Confirm,
    Cancel,
    Submit,
    Complete,
    CursorLeft,
    CursorRight,
    CursorLineStart,
    CursorLineEnd,
    DeleteBackward,
    DeleteForward,
    TimelinePageUp,
    TimelinePageDown,
    TimelineUp,
    TimelineDown,
    TimelineLatest,
    Help,
    Models,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct KeyCombo {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
}

pub struct Keymap {
    bindings: HashMap<String, KeyAction>,
}

impl Keymap {
    pub fn load(cwd: &Path) -> Self {
        let mut keymap = Self::default();
        if let Some(home) = dirs::home_dir() {
            keymap.load_file(&home.join(".piko").join("keybindings.json"));
        }
        keymap.load_file(&cwd.join(".piko").join("keybindings.json"));
        keymap
    }

    pub fn action_for(&self, event: KeyEvent) -> Option<KeyAction> {
        let combo = KeyCombo::from_event(event)?;
        self.bindings.get(&combo.serialize()).copied()
    }

    fn load_file(&mut self, path: &Path) {
        let Ok(content) = fs::read_to_string(path) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return;
        };
        let bindings = value.get("bindings").unwrap_or(&value);
        let Some(bindings) = bindings.as_object() else {
            return;
        };
        for (id, raw) in bindings {
            let Some(action) = action_from_id(id) else {
                continue;
            };
            let Some(raw) = raw.as_str() else {
                continue;
            };
            let Some(combo) = KeyCombo::parse(raw) else {
                continue;
            };
            self.bindings.insert(combo.serialize(), action);
        }
    }

    fn bind(&mut self, raw: &str, action: KeyAction) {
        if let Some(combo) = KeyCombo::parse(raw) {
            self.bindings.insert(combo.serialize(), action);
        }
    }
}

impl Default for Keymap {
    fn default() -> Self {
        let mut keymap = Self {
            bindings: HashMap::new(),
        };
        keymap.bind("ctrl+c", KeyAction::Exit);
        keymap.bind("ctrl+q", KeyAction::Exit);
        keymap.bind("ctrl+n", KeyAction::NewLine);
        keymap.bind("ctrl+r", KeyAction::Sessions);
        keymap.bind("ctrl+k", KeyAction::Commands);
        keymap.bind("ctrl+a", KeyAction::ApprovalAccept);
        keymap.bind("ctrl+s", KeyAction::ApprovalAcceptSession);
        keymap.bind("ctrl+w", KeyAction::ApprovalAcceptWorkspace);
        keymap.bind("ctrl+d", KeyAction::ApprovalDecline);
        keymap.bind("ctrl+l", KeyAction::ClearNotifications);
        keymap.bind("ctrl+p", KeyAction::HistoryPrev);
        keymap.bind("ctrl+e", KeyAction::HistoryNext);
        keymap.bind("up", KeyAction::SelectPrev);
        keymap.bind("down", KeyAction::SelectNext);
        keymap.bind("enter", KeyAction::Submit);
        keymap.bind("esc", KeyAction::Cancel);
        keymap.bind("tab", KeyAction::Complete);
        keymap.bind("left", KeyAction::CursorLeft);
        keymap.bind("right", KeyAction::CursorRight);
        keymap.bind("home", KeyAction::CursorLineStart);
        keymap.bind("end", KeyAction::CursorLineEnd);
        keymap.bind("backspace", KeyAction::DeleteBackward);
        keymap.bind("delete", KeyAction::DeleteForward);
        keymap.bind("pageup", KeyAction::TimelinePageUp);
        keymap.bind("pagedown", KeyAction::TimelinePageDown);
        keymap.bind("f1", KeyAction::Help);
        keymap.bind("f2", KeyAction::Sessions);
        keymap.bind("f3", KeyAction::Models);
        keymap
    }
}

impl KeyCombo {
    fn parse(raw: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;
        for part in raw.to_ascii_lowercase().split('+') {
            match part.trim() {
                "ctrl" | "control" => ctrl = true,
                "alt" | "option" => alt = true,
                "shift" => shift = true,
                "" => {}
                other => key = Some(normalize_key(other)),
            }
        }
        Some(Self {
            key: key?,
            ctrl,
            alt,
            shift,
        })
    }

    fn from_event(event: KeyEvent) -> Option<Self> {
        let key = match event.code {
            KeyCode::Char(ch) => ch.to_ascii_lowercase().to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Esc => "esc".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Delete => "delete".to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::BackTab => "tab".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::PageUp => "pageup".to_string(),
            KeyCode::PageDown => "pagedown".to_string(),
            KeyCode::F(n) => format!("f{n}"),
            _ => return None,
        };
        Some(Self {
            key,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
            alt: event.modifiers.contains(KeyModifiers::ALT),
            shift: event.modifiers.contains(KeyModifiers::SHIFT),
        })
    }

    fn serialize(&self) -> String {
        format!(
            "{}{}{}{}",
            if self.ctrl { "ctrl+" } else { "" },
            if self.alt { "alt+" } else { "" },
            if self.shift { "shift+" } else { "" },
            self.key
        )
    }
}

fn normalize_key(key: &str) -> String {
    match key {
        "escape" => "esc",
        "return" => "enter",
        "pgup" => "pageup",
        "pgdn" => "pagedown",
        other => other,
    }
    .to_string()
}

fn action_from_id(id: &str) -> Option<KeyAction> {
    Some(match id {
        "app.exit" => KeyAction::Exit,
        "tui.input.newLine" => KeyAction::NewLine,
        "app.session.resume" => KeyAction::Sessions,
        "app.session.tree" => KeyAction::SessionTree,
        "app.commands" => KeyAction::Commands,
        "app.settings" => KeyAction::Settings,
        "app.status" => KeyAction::Status,
        "app.approval.accept" => KeyAction::ApprovalAccept,
        "app.approval.acceptSession" => KeyAction::ApprovalAcceptSession,
        "app.approval.acceptWorkspace" => KeyAction::ApprovalAcceptWorkspace,
        "app.approval.decline" => KeyAction::ApprovalDecline,
        "app.notifications.clear" => KeyAction::ClearNotifications,
        "tui.history.prev" => KeyAction::HistoryPrev,
        "tui.history.next" => KeyAction::HistoryNext,
        "tui.select.up" => KeyAction::SelectPrev,
        "tui.select.down" => KeyAction::SelectNext,
        "tui.select.confirm" => KeyAction::Confirm,
        "tui.select.cancel" => KeyAction::Cancel,
        "tui.input.submit" => KeyAction::Submit,
        "tui.input.tab" => KeyAction::Complete,
        "tui.editor.cursorLeft" => KeyAction::CursorLeft,
        "tui.editor.cursorRight" => KeyAction::CursorRight,
        "tui.editor.cursorLineStart" => KeyAction::CursorLineStart,
        "tui.editor.cursorLineEnd" => KeyAction::CursorLineEnd,
        "tui.editor.deleteCharBackward" => KeyAction::DeleteBackward,
        "tui.editor.deleteCharForward" => KeyAction::DeleteForward,
        "tui.timeline.pageUp" => KeyAction::TimelinePageUp,
        "tui.timeline.pageDown" => KeyAction::TimelinePageDown,
        "tui.timeline.up" => KeyAction::TimelineUp,
        "tui.timeline.down" => KeyAction::TimelineDown,
        "tui.timeline.jumpLatest" => KeyAction::TimelineLatest,
        "app.help" => KeyAction::Help,
        "app.model.select" => KeyAction::Models,
        _ => return None,
    })
}
