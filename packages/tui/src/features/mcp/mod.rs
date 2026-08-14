//! MCP server status panel (F-13 client surface).
//!
//! Fed by the `mcp.status` host command: one row per configured server with
//! connection state, tool/resource/template counts, and the connect error
//! when a server failed or timed out at session start.

use piko_protocol::command::McpServerInfo;
use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use crate::app::HitId;
use crate::navigation::{SelectBandBudget, SurfaceId};
use crate::theme::Theme;
use crate::ui::{
    components::{
        pane::PaneSpec,
        selectable_list::{
            ColumnCell, SelectableItem, SelectableList, SelectablePanelBody, paint_row_hover,
            paint_selectable_panel, selectable_row_regions,
        },
    },
    interaction::{ComponentHit, PointerComponent, PointerGesture},
};

impl Component<HitId, Theme> for McpPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx);
        let regions = self.row_regions(area);
        paint_row_hover(frame, &regions, interaction, self.servers.selected, ctx);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.row_regions(area)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect()
    }
}

impl PointerComponent<HitId> for McpPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) if i < self.servers.len() => {
                self.servers.selected = i;
            }
            (PointerGesture::ScrollUp, _) => self.select_prev(),
            (PointerGesture::ScrollDown, _) => self.select_next(),
            _ => {}
        }
        Vec::new()
    }
}

impl SurfacePanel<SurfaceId, HitId, Theme> for McpPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Mcp
    }
}

/// Read-only MCP status panel.
#[derive(Default)]
pub struct McpPanel {
    servers: SelectableList<McpServerInfo>,
}

impl McpPanel {
    fn row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        let items = server_items(&self.servers.items);
        let spec = PaneSpec::new("MCP servers").focused(true);
        selectable_row_regions(area, &spec, &items, self.servers.selected, "")
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_servers(&mut self, servers: Vec<McpServerInfo>) {
        self.servers = SelectableList::new(servers);
    }

    pub fn connected_count(&self) -> usize {
        self.servers.items.iter().filter(|s| s.connected).count()
    }

    #[cfg(test)]
    pub fn selected_index(&self) -> usize {
        self.servers.selected
    }

    pub fn select_band_budget(&self) -> SelectBandBudget {
        let content_rows = if self.servers.is_empty() {
            2
        } else {
            self.servers.len().min(10) as u16
        };
        SelectBandBudget::standard_info(content_rows)
    }

    pub fn interaction_hints(&self) -> crate::ui::interaction_hints::InteractionHints<'static> {
        "↑/↓ browse · Esc close".into()
    }

    pub fn select_next(&mut self) {
        self.servers.select_next("", |_| true);
    }

    pub fn select_prev(&mut self) {
        self.servers.select_prev("", |_| true);
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let items = server_items(&self.servers.items);
        let body = if items.is_empty() {
            SelectablePanelBody::Message(ratatui::widgets::Paragraph::new(
                "No MCP servers configured — see [[mcp-servers]] in settings.",
            ))
        } else {
            SelectablePanelBody::Columns {
                items: &items,
                selected: self.servers.selected,
                widths: None,
            }
        };
        let spec = PaneSpec::new("MCP servers").focused(true);
        let _ = paint_selectable_panel(frame, area, theme, &spec, body);
    }
}

/// Build one display line per server; separated for unit testing.
fn server_items(servers: &[McpServerInfo]) -> Vec<SelectableItem> {
    servers
        .iter()
        .map(|server| {
            if server.connected {
                SelectableItem::columns([
                    ColumnCell::primary(server.name.clone()),
                    ColumnCell::secondary("connected"),
                    ColumnCell::secondary(format!("{} tools", server.tool_count)),
                    ColumnCell::secondary(format!("{} resources", server.resource_count)),
                    ColumnCell::secondary(format!("{} templates", server.template_count)),
                ])
            } else {
                SelectableItem::columns([
                    ColumnCell::primary(server.name.clone()),
                    ColumnCell::secondary("disconnected"),
                    ColumnCell::secondary(
                        server
                            .error
                            .clone()
                            .unwrap_or_else(|| "unknown error".into()),
                    ),
                ])
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
        let lines = server_items(&servers);
        let text = format!("{} {}", lines[0].primary, lines[0].detail);
        assert!(text.contains("github"));
        assert!(text.contains("connected"));
        assert!(text.contains("3 tools"));
    }

    #[test]
    fn disconnected_server_line_shows_error() {
        let servers = [info("slack", false, Some("timeout"))];
        let lines = server_items(&servers);
        let text = format!("{} {}", lines[0].primary, lines[0].detail);
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
        panel.select_next();
        assert_eq!(panel.servers.selected, 1);
        panel.select_prev();
        assert_eq!(panel.servers.selected, 0);
    }
}
