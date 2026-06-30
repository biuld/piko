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

    // Newly added for alignment with pi-mono:
    CursorWordLeft,
    CursorWordRight,
    JumpForward,
    JumpBackward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteToLineStart,
    DeleteToLineEnd,
    Yank,
    YankPop,
    Undo,
    Interrupt,
    Clear,
    Suspend,
    ThinkingCycle,
    ThinkingToggle,
    ToolsExpand,
    SessionToggleNamedFilter,
    EditorExternal,
    MessageFollowUp,
    MessageDequeue,
    ClipboardPasteImage,
    SessionNew,
    SessionFork,
    SessionResume,
    TreeFoldOrUp,
    TreeUnfoldOrDown,
    TreeEditLabel,
    TreeToggleLabelTimestamp,
    SessionTogglePath,
    SessionToggleSort,
    SessionRename,
    SessionDelete,
    SessionDeleteNoninvasive,
    ModelsSave,
    ModelsEnableAll,
    ModelsClearAll,
    ModelsToggleProvider,
    ModelsReorderUp,
    ModelsReorderDown,
    TreeFilterDefault,
    TreeFilterNoTools,
    TreeFilterUserOnly,
    TreeFilterLabeledOnly,
    TreeFilterAll,
    TreeFilterCycleForward,
    TreeFilterCycleBackward,
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
        // Editor Navigation & Editing
        keymap.bind("up", KeyAction::SelectPrev); // Also used in autocomplete list/timeline
        keymap.bind("down", KeyAction::SelectNext); // Also used in autocomplete list/timeline
        keymap.bind("left", KeyAction::CursorLeft);
        keymap.bind("ctrl+b", KeyAction::CursorLeft);
        keymap.bind("right", KeyAction::CursorRight);
        keymap.bind("ctrl+f", KeyAction::CursorRight);
        keymap.bind("alt+left", KeyAction::CursorWordLeft);
        keymap.bind("ctrl+left", KeyAction::CursorWordLeft);
        keymap.bind("alt+b", KeyAction::CursorWordLeft);
        keymap.bind("alt+right", KeyAction::CursorWordRight);
        keymap.bind("ctrl+right", KeyAction::CursorWordRight);
        keymap.bind("alt+f", KeyAction::CursorWordRight);
        keymap.bind("home", KeyAction::CursorLineStart);
        keymap.bind("ctrl+a", KeyAction::CursorLineStart); // Overriden in Models / Tree contexts
        keymap.bind("end", KeyAction::CursorLineEnd);
        keymap.bind("ctrl+e", KeyAction::CursorLineEnd); // Overriden in timeline/history in some contexts
        keymap.bind("ctrl+]", KeyAction::JumpForward);
        keymap.bind("ctrl+alt+]", KeyAction::JumpBackward);
        keymap.bind("pageup", KeyAction::TimelinePageUp);
        keymap.bind("pagedown", KeyAction::TimelinePageDown);
        keymap.bind("backspace", KeyAction::DeleteBackward);
        keymap.bind("delete", KeyAction::DeleteForward);
        keymap.bind("ctrl+d", KeyAction::DeleteForward); // Overriden in Approval / Tree contexts
        keymap.bind("ctrl+w", KeyAction::DeleteWordBackward);
        keymap.bind("alt+backspace", KeyAction::DeleteWordBackward);
        keymap.bind("alt+d", KeyAction::DeleteWordForward);
        keymap.bind("alt+delete", KeyAction::DeleteWordForward);
        keymap.bind("ctrl+u", KeyAction::DeleteToLineStart);
        keymap.bind("ctrl+k", KeyAction::DeleteToLineEnd);
        keymap.bind("ctrl+y", KeyAction::Yank);
        keymap.bind("alt+y", KeyAction::YankPop);
        keymap.bind("ctrl+-", KeyAction::Undo);
        keymap.bind("shift+enter", KeyAction::NewLine);
        keymap.bind("ctrl+j", KeyAction::NewLine);
        keymap.bind("enter", KeyAction::Submit);
        keymap.bind("tab", KeyAction::Complete);
        keymap.bind("ctrl+c", KeyAction::Clear); // Clears editor in pi-mono, also mapped to Selection Cancel
        keymap.bind("esc", KeyAction::Cancel); // Maps to global Cancel / abort / panel close

        // Generic selection defaults
        keymap.bind("pageup", KeyAction::TimelinePageUp);
        keymap.bind("pagedown", KeyAction::TimelinePageDown);

        // Application Actions & Overlays
        keymap.bind("ctrl+q", KeyAction::Exit);
        keymap.bind("ctrl+z", KeyAction::Suspend);
        keymap.bind("shift+tab", KeyAction::ThinkingCycle);
        keymap.bind("ctrl+p", KeyAction::HistoryPrev); // Overriden to cycle model or filter tree in specific panels
        keymap.bind("ctrl+e", KeyAction::HistoryNext);
        keymap.bind("ctrl+l", KeyAction::Models); // Open model selector
        keymap.bind("ctrl+o", KeyAction::ToolsExpand);
        keymap.bind("ctrl+t", KeyAction::ThinkingToggle);
        keymap.bind("ctrl+n", KeyAction::SessionToggleNamedFilter);
        keymap.bind("ctrl+g", KeyAction::EditorExternal);
        keymap.bind("alt+enter", KeyAction::MessageFollowUp);
        keymap.bind("alt+up", KeyAction::MessageDequeue);
        keymap.bind("ctrl+v", KeyAction::ClipboardPasteImage);
        keymap.bind("alt+v", KeyAction::ClipboardPasteImage);

        // Functional keys
        keymap.bind("f1", KeyAction::Help);
        keymap.bind("f2", KeyAction::SessionTree);
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
        "app.interrupt" => KeyAction::Interrupt,
        "app.clear" => KeyAction::Clear,
        "app.suspend" => KeyAction::Suspend,
        "app.thinking.cycle" => KeyAction::ThinkingCycle,
        "app.thinking.toggle" => KeyAction::ThinkingToggle,
        "app.tools.expand" => KeyAction::ToolsExpand,
        "app.session.toggleNamedFilter" => KeyAction::SessionToggleNamedFilter,
        "app.editor.external" => KeyAction::EditorExternal,
        "app.message.followUp" => KeyAction::MessageFollowUp,
        "app.message.dequeue" => KeyAction::MessageDequeue,
        "app.clipboard.pasteImage" => KeyAction::ClipboardPasteImage,
        "app.session.new" => KeyAction::SessionNew,
        "app.session.fork" => KeyAction::SessionFork,
        "app.session.resume" => KeyAction::Sessions,
        "app.session.resume_action" => KeyAction::SessionResume,
        "app.tree.foldOrUp" => KeyAction::TreeFoldOrUp,
        "app.tree.unfoldOrDown" => KeyAction::TreeUnfoldOrDown,
        "app.tree.editLabel" => KeyAction::TreeEditLabel,
        "app.tree.toggleLabelTimestamp" => KeyAction::TreeToggleLabelTimestamp,
        "app.session.togglePath" => KeyAction::SessionTogglePath,
        "app.session.toggleSort" => KeyAction::SessionToggleSort,
        "app.session.rename" => KeyAction::SessionRename,
        "app.session.delete" => KeyAction::SessionDelete,
        "app.session.deleteNoninvasive" => KeyAction::SessionDeleteNoninvasive,
        "app.models.save" => KeyAction::ModelsSave,
        "app.models.enableAll" => KeyAction::ModelsEnableAll,
        "app.models.clearAll" => KeyAction::ModelsClearAll,
        "app.models.toggleProvider" => KeyAction::ModelsToggleProvider,
        "app.models.reorderUp" => KeyAction::ModelsReorderUp,
        "app.models.reorderDown" => KeyAction::ModelsReorderDown,
        "app.tree.filter.default" => KeyAction::TreeFilterDefault,
        "app.tree.filter.noTools" => KeyAction::TreeFilterNoTools,
        "app.tree.filter.userOnly" => KeyAction::TreeFilterUserOnly,
        "app.tree.filter.labeledOnly" => KeyAction::TreeFilterLabeledOnly,
        "app.tree.filter.all" => KeyAction::TreeFilterAll,
        "app.tree.filter.cycleForward" => KeyAction::TreeFilterCycleForward,
        "app.tree.filter.cycleBackward" => KeyAction::TreeFilterCycleBackward,
        "app.approval.accept" => KeyAction::ApprovalAccept,
        "app.approval.acceptSession" => KeyAction::ApprovalAcceptSession,
        "app.approval.acceptWorkspace" => KeyAction::ApprovalAcceptWorkspace,
        "app.approval.decline" => KeyAction::ApprovalDecline,

        "tui.input.newLine" => KeyAction::NewLine,
        "tui.input.submit" => KeyAction::Submit,
        "tui.input.tab" => KeyAction::Complete,
        "tui.input.copy" => KeyAction::Clear, // maps to clear/cancel in editor

        "tui.select.up" => KeyAction::SelectPrev,
        "tui.select.down" => KeyAction::SelectNext,
        "tui.select.pageUp" => KeyAction::TimelinePageUp,
        "tui.select.pageDown" => KeyAction::TimelinePageDown,
        "tui.select.confirm" => KeyAction::Confirm,
        "tui.select.cancel" => KeyAction::Cancel,

        "tui.editor.cursorUp" => KeyAction::SelectPrev,
        "tui.editor.cursorDown" => KeyAction::SelectNext,
        "tui.editor.cursorLeft" => KeyAction::CursorLeft,
        "tui.editor.cursorRight" => KeyAction::CursorRight,
        "tui.editor.cursorWordLeft" => KeyAction::CursorWordLeft,
        "tui.editor.cursorWordRight" => KeyAction::CursorWordRight,
        "tui.editor.cursorLineStart" => KeyAction::CursorLineStart,
        "tui.editor.cursorLineEnd" => KeyAction::CursorLineEnd,
        "tui.editor.jumpForward" => KeyAction::JumpForward,
        "tui.editor.jumpBackward" => KeyAction::JumpBackward,
        "tui.editor.pageUp" => KeyAction::TimelinePageUp,
        "tui.editor.pageDown" => KeyAction::TimelinePageDown,
        "tui.editor.deleteCharBackward" => KeyAction::DeleteBackward,
        "tui.editor.deleteCharForward" => KeyAction::DeleteForward,
        "tui.editor.deleteWordBackward" => KeyAction::DeleteWordBackward,
        "tui.editor.deleteWordForward" => KeyAction::DeleteWordForward,
        "tui.editor.deleteToLineStart" => KeyAction::DeleteToLineStart,
        "tui.editor.deleteToLineEnd" => KeyAction::DeleteToLineEnd,
        "tui.editor.yank" => KeyAction::Yank,
        "tui.editor.yankPop" => KeyAction::YankPop,
        "tui.editor.undo" => KeyAction::Undo,

        "tui.history.prev" => KeyAction::HistoryPrev,
        "tui.history.next" => KeyAction::HistoryNext,
        "tui.timeline.pageUp" => KeyAction::TimelinePageUp,
        "tui.timeline.pageDown" => KeyAction::TimelinePageDown,
        "tui.timeline.up" => KeyAction::TimelineUp,
        "tui.timeline.down" => KeyAction::TimelineDown,
        "tui.timeline.jumpLatest" => KeyAction::TimelineLatest,

        "app.session.tree" => KeyAction::SessionTree,
        "app.commands" => KeyAction::Commands,
        "app.settings" => KeyAction::Settings,
        "app.status" => KeyAction::Status,
        "app.notifications.clear" => KeyAction::ClearNotifications,
        "app.help" => KeyAction::Help,
        "app.model.select" => KeyAction::Models,
        _ => return None,
    })
}
