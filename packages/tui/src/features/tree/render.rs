use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use piko_tui_layout::{Component, InteractionState, SurfacePanel};

use super::{ConnectorKind, TreeFilterMode, TreePanel, visible};
use crate::app::{HitId, command::SurfaceAction};
use crate::navigation::SurfaceId;
use crate::theme::Theme;
use crate::ui::components::choice_workflow::ChoiceWorkflow;
use crate::ui::components::pane::{PaneAffixHit, PaneFooter, PaneSearch, PaneSpec, PaneTitleAffix};
use crate::ui::components::selectable_list::{
    ColumnCell, SelectableItem, SelectablePanelBody, paint_row_hover, paint_selectable_panel,
    selectable_row_regions,
};
use crate::ui::components::selection_prefix;
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture, paint_element_hover};

/// Render context for the session-tree surface (including its summary prompt).
pub struct TreeCtx<'a> {
    pub filter: &'a str,
    pub summary_prompt: Option<&'a ChoiceWorkflow>,
    pub theme: &'a Theme,
    pub tip: Option<&'a str>,
    pub hints: Option<&'a str>,
}

impl Component<HitId, TreeCtx<'_>> for TreePanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &TreeCtx<'_>) {
        TreePanel::render_with_context(self, frame, area, ctx);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &TreeCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        TreePanel::render_with_context(self, frame, area, ctx);
        if let Some(workflow) = ctx.summary_prompt {
            if let Some(footer) = self.summary_footer_rect(area) {
                workflow.render_embedded_hover(frame, footer, ctx.theme, interaction);
            }
        } else {
            let regions = self.row_regions(area, false);
            paint_row_hover(frame, &regions, interaction, self.selected_idx, ctx.theme);
            paint_element_hover(
                frame,
                &self.title_regions(area),
                interaction,
                Some(HitId::Mode(self.mode_index())),
                ctx.theme,
            );
            paint_element_hover(
                frame,
                &self.label_input_region(area),
                interaction,
                None,
                ctx.theme,
            );
        }
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let mut regions: Vec<_> = self
            .row_regions(area, false)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect();
        regions.extend(self.title_regions(area));
        regions.extend(self.label_input_region(area));
        regions
    }
}

impl PointerComponent<HitId> for TreePanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) if i < self.visible.rows.len() => {
                self.selected_idx = i;
                self.selection = Some(self.visible.rows[i].entry_id.clone());
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::Activate, Some(HitId::Mode(i))) => {
                let modes = [
                    TreeFilterMode::Default,
                    TreeFilterMode::NoTools,
                    TreeFilterMode::UserOnly,
                    TreeFilterMode::LabeledOnly,
                    TreeFilterMode::All,
                ];
                if let Some(mode) = modes.get(i).copied() {
                    self.toggle_filter_for_current_search(mode);
                }
                Vec::new()
            }
            (PointerGesture::Activate, Some(HitId::TextInput)) => {
                if let Some(editor) = &mut self.label_editor {
                    editor.input.move_to_column(hit.local_x());
                }
                Vec::new()
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev_filtered();
                Vec::new()
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next_filtered();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl SurfacePanel<SurfaceId, HitId, TreeCtx<'_>> for TreePanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Tree
    }
}

impl TreePanel {
    fn label_input_region(&self, area: Rect) -> Vec<(Rect, HitId)> {
        if self.label_editor.is_none() {
            return Vec::new();
        }
        let prefix = "Label: ";
        let spec = PaneSpec::new("Session Tree")
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .search(PaneSearch::Custom(Line::default()));
        spec.search_rect(area)
            .map(|search| {
                let offset = prefix.chars().count() as u16;
                vec![(
                    Rect::new(
                        search.x.saturating_add(offset),
                        search.y,
                        search.width.saturating_sub(offset),
                        1,
                    ),
                    HitId::TextInput,
                )]
            })
            .unwrap_or_default()
    }

    fn mode_index(&self) -> usize {
        match self.filter_mode {
            TreeFilterMode::Default => 0,
            TreeFilterMode::NoTools => 1,
            TreeFilterMode::UserOnly => 2,
            TreeFilterMode::LabeledOnly => 3,
            TreeFilterMode::All => 4,
        }
    }

    fn title_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        PaneSpec::new("Session Tree")
            .title_affixes([
                PaneTitleAffix::mode_strip_static(
                    &["Default", "NoTools", "User", "Labeled", "All"],
                    self.mode_index(),
                ),
                PaneTitleAffix::selection(
                    if self.visible.rows.is_empty() {
                        0
                    } else {
                        self.selected_idx + 1
                    },
                    self.visible.rows.len(),
                ),
            ])
            .title_affix_regions(area)
            .into_iter()
            .filter_map(|(rect, hit)| match hit {
                PaneAffixHit::ModeOption(i) => Some((rect, HitId::Mode(i))),
                PaneAffixHit::Close => None,
            })
            .collect()
    }

    pub(crate) fn summary_footer_rect(&self, area: Rect) -> Option<Rect> {
        let spec = PaneSpec::new("Session Tree")
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .search_filter(&self.filter)
            .tip(" ")
            .footer(PaneFooter::Reserved { height: 7 });
        spec.footer_rect(area)
    }

    pub(crate) fn row_regions(&self, area: Rect, summary_prompt: bool) -> Vec<(Rect, usize)> {
        let mode_active = self.mode_index();
        let footer = if summary_prompt {
            PaneFooter::Reserved { height: 7 }
        } else {
            // Hit testing only needs the one-row footer budget. Render text is
            // supplied by the binding-derived context.
            PaneFooter::Reserved { height: 1 }
        };
        let title = if self.show_label_timestamps {
            "Session Tree [+time]"
        } else {
            "Session Tree"
        };
        let spec = PaneSpec::new(title)
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .title_affixes([
                PaneTitleAffix::mode_strip_static(
                    &["Default", "NoTools", "User", "Labeled", "All"],
                    mode_active,
                ),
                PaneTitleAffix::selection(
                    if self.visible.rows.is_empty() {
                        0
                    } else {
                        self.selected_idx + 1
                    },
                    self.visible.rows.len(),
                ),
            ])
            .search(PaneSearch::Shown {
                filter: &self.filter,
                placeholder: None,
            })
            .tip(" ")
            .footer(footer)
            .focused(true);
        let items: Vec<SelectableItem> = self
            .visible
            .rows
            .iter()
            .map(|row| SelectableItem::columns([ColumnCell::primary(row.entry_id.clone())]))
            .collect();
        selectable_row_regions(area, &spec, &items, self.selected_idx, "")
    }
    fn render_with_context(&self, frame: &mut Frame<'_>, area: Rect, ctx: &TreeCtx<'_>) {
        let filter = ctx.filter;
        let summary_prompt = ctx.summary_prompt;
        let theme = ctx.theme;
        let tip = ctx.tip;
        let hints = ctx.hints;
        let mut left_title = "Session Tree".to_string();
        if self.show_label_timestamps {
            left_title.push_str(" [+time]");
        }

        let mode_active = match self.filter_mode {
            TreeFilterMode::Default => 0,
            TreeFilterMode::NoTools => 1,
            TreeFilterMode::UserOnly => 2,
            TreeFilterMode::LabeledOnly => 3,
            TreeFilterMode::All => 4,
        };

        let (sel_at, sel_of) = if self.visible.rows.is_empty() {
            (0, 0)
        } else {
            (
                self.selected_idx
                    .saturating_add(1)
                    .min(self.visible.rows.len()),
                self.visible.rows.len(),
            )
        };

        let search = if let Some(editor) = &self.label_editor {
            let mut spans = vec![Span::styled("Label: ", Style::default().fg(theme.accent))];
            let tb_line = editor.input.render_line(theme, true);
            spans.extend(tb_line.spans);
            PaneSearch::Custom(Line::from(spans))
        } else {
            PaneSearch::Shown {
                filter,
                placeholder: None,
            }
        };

        let lines: Vec<Line<'static>> = self
            .visible
            .rows
            .iter()
            .enumerate()
            .map(|(idx, row)| self.row_line(row, idx == self.selected_idx, theme))
            .collect();

        let footer = if summary_prompt.is_some() {
            PaneFooter::Reserved { height: 7 }
        } else {
            hints
                .filter(|value| !value.is_empty())
                .map_or(PaneFooter::Reserved { height: 1 }, |value| {
                    PaneFooter::Hints(value.into())
                })
        };

        let spec = PaneSpec::new(&left_title)
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .title_affixes([
                PaneTitleAffix::mode_strip_static(
                    &["Default", "NoTools", "User", "Labeled", "All"],
                    mode_active,
                ),
                PaneTitleAffix::selection(sel_at, sel_of),
            ])
            .search(search)
            .tip(tip.or(Some(" ")))
            .footer(footer)
            .focused(true);

        let body = if lines.is_empty() {
            let msg = if filter.is_empty() {
                "No entries found."
            } else {
                "No entries match the filter."
            };
            SelectablePanelBody::Message(
                Paragraph::new(msg).style(Style::default().fg(theme.muted)),
            )
        } else {
            SelectablePanelBody::RichLines {
                lines: &lines,
                selected: self.selected_idx,
            }
        };

        let Some(areas) = paint_selectable_panel(frame, area, theme, &spec, body) else {
            return;
        };

        if let (Some(footer_area), Some(state)) = (areas.footer, summary_prompt) {
            state.render(frame, footer_area, theme);
        }
    }

    fn row_line(&self, row: &visible::TreeRow, is_selected: bool, theme: &Theme) -> Line<'static> {
        let bg = if is_selected {
            theme.bg_selected
        } else {
            Color::Reset
        };
        let styled = |text: String, color: Color| {
            let style = Style::default().fg(color).bg(bg);
            let style = if is_selected {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            };
            Span::styled(text, style)
        };

        let mut spans = Vec::new();
        spans.push(styled(selection_prefix(is_selected), theme.accent));

        let prefix = tree_row_prefix(row);
        if !prefix.is_empty() {
            spans.push(styled(prefix, theme.dim));
        }

        if row.is_folded {
            spans.push(styled("⊞ ".to_string(), theme.accent));
        }
        if row.is_active_path {
            spans.push(styled("• ".to_string(), theme.accent));
        }

        if let Some(label) = &row.label {
            let label_text = if self.show_label_timestamps {
                format!(
                    "[{} - {}] ",
                    label.text.as_deref().unwrap_or(""),
                    label.timestamp
                )
            } else {
                format!("[{}] ", label.text.as_deref().unwrap_or(""))
            };
            spans.push(styled(label_text, theme.warning));
        }

        spans.push(styled(
            row.role_preview.clone(),
            self.role_color(row, theme),
        ));
        spans.push(styled(" ".to_string(), theme.text));
        spans.push(styled(row.text_preview.clone(), theme.text));

        Line::from(spans)
    }

    fn role_color(&self, row: &visible::TreeRow, theme: &Theme) -> Color {
        if row.role_preview.contains("assistant") {
            theme.success
        } else if row.role_preview.contains("user") {
            theme.accent
        } else if row.role_preview.contains("branch") || row.role_preview.contains("compact") {
            theme.warning
        } else {
            theme.dim
        }
    }
}

pub(crate) fn tree_row_prefix(row: &visible::TreeRow) -> String {
    let connector_position = match row.connector {
        ConnectorKind::Branch | ConnectorKind::Corner => row.depth.checked_sub(1),
        ConnectorKind::Vertical | ConnectorKind::None => None,
    };
    let mut prefix = String::new();
    for level in 0..row.depth {
        if Some(level) == connector_position {
            match row.connector {
                ConnectorKind::Branch => prefix.push_str("├─ "),
                ConnectorKind::Corner => prefix.push_str("└─ "),
                _ => prefix.push_str("   "),
            }
        } else if row
            .gutters
            .iter()
            .any(|g| g.position == level && g.kind == ConnectorKind::Vertical)
        {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }
    prefix
}

#[cfg(test)]
mod tests;
