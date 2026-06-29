use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use piko_protocol::CommandCatalogItem;

use crate::theme::Theme;

/// Help panel: static keybinding reference.
pub struct HelpPanel;

impl HelpPanel {
    pub fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        commands: &[CommandCatalogItem],
    ) {
        frame.render_widget(Clear, area);
        let mut lines = vec![
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
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        for command in commands
            .iter()
            .filter(|command| !command.slash_names.is_empty())
        {
            let names = command.slash_names.join(", ");
            lines.push(format!("  {names:<18} {}", command.detail));
        }
        lines.extend(
            [
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
            .into_iter()
            .map(str::to_string),
        );
        let text = lines.join("\n");
        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_muted))
                    .title("help"),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }
}
