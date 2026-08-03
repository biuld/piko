//! MCP server status panel (F-13 client surface).
//!
//! Fed by the `mcp.status` host command: one row per configured server with
//! connection state, tool/resource/template counts, and the connect error
//! when a server failed or timed out at session start.

use piko_protocol::command::McpServerInfo;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::theme::Theme;

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

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let popup = centered_rect(74, 55, area);
        frame.render_widget(Clear, popup);

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
        text.push(Line::from(""));
        text.push(Line::from(" Esc close"));

        let block = Block::default()
            .title(" MCP servers ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border));
        let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, popup);
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
        let servers = [info("filesystem", true, None)];
        let lines = server_lines(&servers);
        let line = lines[0].to_string();
        assert!(line.contains("filesystem"));
        assert!(line.contains("connected"));
        assert!(line.contains("3 tools"));
        assert!(line.contains("1 resources"));
        assert!(line.contains("2 templates"));
    }

    #[test]
    fn disconnected_server_line_shows_error() {
        let servers = [info("hang", false, Some("timed out after 10000 ms"))];
        let lines = server_lines(&servers);
        let line = lines[0].to_string();
        assert!(line.contains("hang"));
        assert!(line.contains("disconnected"));
        assert!(line.contains("timed out after 10000 ms"));
    }

    #[test]
    fn panel_tracks_connected_count() {
        let mut panel = McpPanel::new();
        panel.set_servers(vec![
            info("a", true, None),
            info("b", false, Some("boom")),
            info("c", true, None),
        ]);
        assert_eq!(panel.connected_count(), 2);
        assert_eq!(panel.servers().len(), 3);
    }
}
