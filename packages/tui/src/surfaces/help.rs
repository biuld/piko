use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Help overlay: static keybinding reference.
pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let text = [
            "Core",
            "  Enter              submit input",
            "  Ctrl-N             insert newline",
            "  Left/Right/Home/End edit input",
            "  Backspace/Delete   edit input",
            "  Tab                accept command/file suggestion",
            "  Ctrl-P/Ctrl-E      input history previous/next",
            "  Esc                cancel active turn",
            "  PgUp/PgDn, Up/Down scroll timeline",
            "",
            "Surfaces",
            "  Ctrl-K or /commands open command palette",
            "  F2 or /sessions    list and open sessions",
            "  /tree              inspect current session branch tree",
            "  F3 or /models      list and set default model",
            "  /settings          open hostd-backed runtime settings",
            "  /status            show turn, queue, approval, and tool state",
            "  F1 or /help        show help",
            "  ~/.piko/keybindings.json and .piko/keybindings.json override keys",
            "",
            "Commands",
            "  /new               create a new session",
            "  /fork [entry_id]   fork current session at a tree entry",
            "  /clone             clone current session at current leaf",
            "  /name <name>       rename current session",
            "  /import <path>     import a session JSONL file",
            "  /delete confirm    delete current session",
            "  /login [provider]  start OAuth login",
            "  /logout [provider] remove credentials",
            "  /compact           compact current session",
            "",
            "Approvals",
            "  Ctrl-A             accept current request once",
            "  Ctrl-S             accept current request for session",
            "  Ctrl-W             accept current request for workspace",
            "  Ctrl-D             decline current request",
            "  Ctrl-L             clear notifications",
            "",
            "Press Esc, Enter, or q to close this panel.",
        ]
        .join("\n");
        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("help"))
            .wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }
}
