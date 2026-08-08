use ratatui::{Frame, layout::Rect};

use crate::app::command::TuiCommandEntry;
use crate::theme::Theme;
use crate::ui::components::pane::{PaneSpec, render_text_pane};

/// Help panel: static keybinding reference.
pub struct HelpPanel;

impl HelpPanel {
    pub fn render(frame: &mut Frame<'_>, area: Rect, theme: &Theme, commands: &[TuiCommandEntry]) {
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
            "  F2 or /resume      list and open sessions",
            "  /tree              inspect current session branch tree",
            "  F3 or /models      list and set default model",
            "  /thinking          list and set default thinking level",
            "  /settings          open hostd-backed runtime settings",
            "  /status            show turn, queue, approval, and tool state",
            "  /diff              show workspace diff for last/active turn",
            "  /prompt-debug      show latest prompt assembly diagnostics",
            "  /rollout           page active agent rollout transcript",
            "  /mcp               show connected MCP servers, tools, and resources",
            "  F1 or /help        show help",
            "  ~/.piko/keybindings.json and .piko/keybindings.json override keys",
            "",
            "Commands",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        for command in commands {
            let name = &command.slash;
            lines.push(format!("  {name:<18} {}", command.detail));
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
            ]
            .into_iter()
            .map(str::to_string),
        );
        let text = lines.join("\n");
        let spec = PaneSpec::new("help")
            .hints("Esc | Enter | q close")
            .focused(true);
        render_text_pane(frame, area, &spec, text, theme);
    }
}
