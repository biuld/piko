//! MCP server status panel (F-13 client surface).
//!
//! Fed by the `mcp.status` host command: one row per configured server with
//! connection state, tool/resource/template counts, and the connect error
//! when a server failed or timed out at session start.

use piko_protocol::command::McpServerInfo;
use ratatui::{Frame, layout::Rect, text::Line};

use crate::navigation::SelectBandBudget;
use crate::theme::Theme;
use crate::ui::components::pane::{PaneSpec, render_pane};
use ratatui::widgets::Paragraph;

use super::centered_rect;

/// Read-only MCP status panel.
#[derive(Default)]
pub struct McpPanel {
    servers: Vec<McpServerInfo>,
}

impl McpPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_servers(&mut self, servers: Vec<McpServerInfo>) {
        self.servers = servers;
    }

    pub fn servers(&self) -> &[McpServerInfo] {
        &self.servers
    }

    pub fn connected_count(&self) -> usize {
        self.servers.iter().filter(|s| s.connected).count()
    }

    /// ComposerBand content-row budget (summary + one line per server).
    pub fn select_band_budget(&self) -> SelectBandBudget {
        let body_lines = if self.servers.is_empty() {
            2 // summary + empty hint
        } else {
            1 + self.servers.len() // summary + rows; overflow scrolls via tall band clamp
        };
        SelectBandBudget::standard_info(body_lines.min(12) as u16)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let popup = centered_rect(74, 55, area);

        let mut text = vec![Line::from(format!(
            "{} server(s) configured",
            self.servers.len()
        ))];
        let lines = server_lines(&self.servers);
        if lines.is_empty() {
            text.push(Line::from(
                "  (no MCP servers configured — see [[mcp-servers]] in settings)",
            ));
        } else {
            text.extend(lines);
        }

        let spec = PaneSpec::new("MCP servers")
            .hints("Esc close")
            .focused(true);
        if let Some(areas) = render_pane(frame, popup, &spec, theme) {
            frame.render_widget(Paragraph::new(text), areas.content);
        }
    }
}

/// Build one display line per server; separated for unit testing.
fn server_lines(servers: &[McpServerInfo]) -> Vec<Line<'_>> {
    servers
        .iter()
        .map(|server| {
            if server.connected {
                Line::from(format!(
                    "  {}  connected  {} tools · {} resources · {} templates",
                    server.name, server.tool_count, server.resource_count, server.template_count
                ))
            } else {
                Line::from(format!(
                    "  {}  disconnected: {}",
                    server.name,
                    server.error.as_deref().unwrap_or("unknown error")
                ))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, connected: bool, error: Option<&str>) -> McpServerInfo {
        McpServerInfo {
            name: name.into(),
            connected,
            tool_count: if connected { 3 } else { 0 },
            resource_count: if connected { 1 } else { 0 },
            template_count: if connected { 2 } else { 0 },
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn connected_server_line_renders_counts() {
        let servers = [info("github", true, None)];
        let lines = server_lines(&servers);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("github"));
        assert!(text.contains("connected"));
        assert!(text.contains("3 tools"));
    }

    #[test]
    fn disconnected_server_line_shows_error() {
        let servers = [info("slack", false, Some("timeout"))];
        let lines = server_lines(&servers);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("disconnected"));
        assert!(text.contains("timeout"));
    }

    #[test]
    fn panel_tracks_connected_count() {
        let mut panel = McpPanel::new();
        panel.set_servers(vec![
            info("a", true, None),
            info("b", false, Some("err")),
            info("c", true, None),
        ]);
        assert_eq!(panel.connected_count(), 2);
        assert_eq!(panel.servers().len(), 3);
    }
}
