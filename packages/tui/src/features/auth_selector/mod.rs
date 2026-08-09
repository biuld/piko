use ratatui::{
    Frame,
    layout::{Position, Rect},
};
use unicode_width::UnicodeWidthStr;

use piko_tui_layout::{Component, InteractionState, SurfacePanel};

use crate::{
    app::{HitId, command::SurfaceAction},
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::components::{
        menu::{MenuConfirmResult, MenuRow, MenuRowKind, MenuRowLayout, MenuStack},
        selectable_list::{minimal_row_regions, paint_row_hover},
        text_box::TextBox,
    },
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

impl Component<HitId, Theme> for AuthSelector {
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
        match self.state {
            AuthSelectorState::Menu => {
                let regions = self.menu_row_regions(area);
                paint_row_hover(
                    frame,
                    &regions,
                    interaction,
                    self.menu.selected_index(),
                    ctx,
                );
            }
            AuthSelectorState::ApiKeyInput { .. } => {
                if interaction.hovered == Some(HitId::TextInput)
                    && let Some(bg) = crate::ui::components::hover_bg(ctx)
                    && let Some(rect) = self.input_rect(area)
                {
                    frame
                        .buffer_mut()
                        .set_style(rect, ratatui::style::Style::default().bg(bg));
                }
            }
        }
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        match self.state {
            AuthSelectorState::Menu => self
                .menu_row_regions(area)
                .into_iter()
                .map(|(r, i)| (r, HitId::Row(i)))
                .collect(),
            AuthSelectorState::ApiKeyInput { .. } => self
                .input_rect(area)
                .map(|r| vec![(r, HitId::TextInput)])
                .unwrap_or_default(),
        }
    }
}

impl PointerComponent<HitId> for AuthSelector {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i)))
                if matches!(self.state, AuthSelectorState::Menu) =>
            {
                self.menu.select_index(i);
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::Activate, Some(HitId::TextInput)) => {
                if let AuthSelectorState::ApiKeyInput { input, .. } = &mut self.state {
                    input.move_to_column(hit.local_x());
                }
                Vec::new()
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev();
                Vec::new()
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl SurfacePanel<SurfaceId, HitId, Theme> for AuthSelector {
    fn region(&self) -> SurfaceId {
        SurfaceId::AuthSelector
    }
}

#[derive(Clone, Debug)]
pub enum AuthAction {
    StartOAuth { provider: String },
    StartApiKey { provider: String },
}

pub enum AuthSelectorState {
    Menu,
    ApiKeyInput { provider: String, input: TextBox },
}

pub enum AuthConfirmResult {
    StartOAuth { provider: String },
    StartApiKeyInput,
    SetApiKey { provider: String, api_key: String },
    None,
}

pub struct AuthSelector {
    pub state: AuthSelectorState,
    pub menu: MenuStack<AuthAction>,
    pub filter: String,
}

impl AuthSelector {
    fn menu_row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        let title = self
            .menu
            .current()
            .map(|f| f.title.as_str())
            .unwrap_or("authentication");
        minimal_row_regions(
            area,
            title,
            &self.menu.display_items(),
            self.menu.selected_index(),
            &self.filter,
        )
    }

    fn input_rect(&self, area: Rect) -> Option<Rect> {
        let AuthSelectorState::ApiKeyInput { provider, .. } = &self.state else {
            return None;
        };
        let title = format!("Configure {provider} API Key");
        let spec = crate::ui::components::pane::PaneSpec::minimal(&title)
            .hints("Enter save · Esc back")
            .focused(true);
        let content = spec.content_rect(area)?;
        let label_width = "Enter API key: ".chars().count() as u16;
        Some(Rect::new(
            content.x.saturating_add(label_width),
            content.y,
            content.width.saturating_sub(label_width),
            1,
        ))
    }
    pub fn new(available_providers: &[String], authenticated: &[String]) -> Self {
        let rows = Self::build_menu_tree(available_providers, authenticated);
        let mut menu = MenuStack::new();
        menu.open("authentication", MenuRowLayout::Stacked, rows);
        Self {
            state: AuthSelectorState::Menu,
            menu,
            filter: String::new(),
        }
    }

    pub fn build_menu_tree(
        available_providers: &[String],
        authenticated: &[String],
    ) -> Vec<MenuRow<AuthAction>> {
        let oauth_providers = vec!["openai".to_string()];
        let mut api_key_providers = vec![
            "anthropic".to_string(),
            "openai".to_string(),
            "deepseek".to_string(),
        ];

        // Merge dynamically discovered providers from hostd
        for p in available_providers {
            if !api_key_providers.contains(p) {
                api_key_providers.push(p.clone());
            }
        }

        let oauth_children = oauth_providers
            .into_iter()
            .map(|p| {
                let (title, detail) = match p.as_str() {
                    "openai" => (
                        "OpenAI (Subscription)".to_string(),
                        "Authenticate ChatGPT Plus/Pro".to_string(),
                    ),
                    _ => (p.clone(), format!("OAuth login for {p}")),
                };
                action_row(
                    title,
                    detail,
                    p,
                    |provider| AuthAction::StartOAuth { provider },
                    authenticated,
                )
            })
            .collect();

        let api_key_children = api_key_providers
            .into_iter()
            .map(|p| {
                let (title, detail) = match p.as_str() {
                    "anthropic" => (
                        "Anthropic (Claude)".to_string(),
                        "Set Anthropic API key".to_string(),
                    ),
                    "openai" => ("OpenAI (GPT)".to_string(), "Set OpenAI API key".to_string()),
                    "deepseek" => ("DeepSeek".to_string(), "Set DeepSeek API key".to_string()),
                    _ => (p.clone(), format!("Set API key for {p}")),
                };
                action_row(
                    title,
                    detail,
                    p,
                    |provider| AuthAction::StartApiKey { provider },
                    authenticated,
                )
            })
            .collect();

        // Root frame lists the two auth methods directly (frame title is the
        // panel title); no extra single-row "authentication" level.
        vec![
            group_row(
                "Use a subscription (OAuth)",
                "Log in using a web browser subscription",
                oauth_children,
            ),
            group_row(
                "Use an API Key",
                "Manually configure an API key",
                api_key_children,
            ),
        ]
    }

    pub fn reset(&mut self, available_providers: &[String], authenticated: &[String]) {
        self.state = AuthSelectorState::Menu;
        self.filter.clear();
        let rows = Self::build_menu_tree(available_providers, authenticated);
        self.menu
            .open("authentication", MenuRowLayout::Stacked, rows);
    }

    /// ComposerBand content-row budget (stacked menu or fixed form).
    pub fn select_band_budget(&self) -> SelectBandBudget {
        match &self.state {
            AuthSelectorState::Menu => {
                SelectBandBudget::minimal_stacked_list(self.menu.filtered_item_count(&self.filter))
            }
            AuthSelectorState::ApiKeyInput { .. } => {
                // input line + blank + note (matches render body)
                SelectBandBudget::minimal_form(3)
            }
        }
    }

    pub fn select_next(&mut self) {
        if let AuthSelectorState::Menu = self.state {
            self.menu.select_next(&self.filter);
        }
    }

    pub fn select_prev(&mut self) {
        if let AuthSelectorState::Menu = self.state {
            self.menu.select_prev(&self.filter);
        }
    }

    pub fn confirm(&mut self) -> AuthConfirmResult {
        match &mut self.state {
            AuthSelectorState::Menu => match self.menu.confirm(&mut self.filter) {
                MenuConfirmResult::Apply(AuthAction::StartOAuth { provider }) => {
                    AuthConfirmResult::StartOAuth { provider }
                }
                MenuConfirmResult::Apply(AuthAction::StartApiKey { provider }) => {
                    self.state = AuthSelectorState::ApiKeyInput {
                        provider,
                        input: TextBox::new()
                            .with_mask('•')
                            .with_placeholder("Paste API key here..."),
                    };
                    self.filter.clear();
                    AuthConfirmResult::StartApiKeyInput
                }
                MenuConfirmResult::Drilled | MenuConfirmResult::None => AuthConfirmResult::None,
            },
            AuthSelectorState::ApiKeyInput { provider, input } => AuthConfirmResult::SetApiKey {
                provider: provider.clone(),
                api_key: input.text().to_string(),
            },
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        match &self.state {
            AuthSelectorState::Menu => {
                self.menu.render_minimal(frame, area, &self.filter, theme);
            }
            AuthSelectorState::ApiKeyInput { provider, input } => {
                use crate::ui::components::pane::{PaneSpec, render_pane};
                use ratatui::style::Style;
                use ratatui::text::{Line, Span};
                use ratatui::widgets::Paragraph;

                let title = format!("Configure {provider} API Key");
                let spec = PaneSpec::minimal(&title)
                    .hints("Enter save · Esc back")
                    .focused(true);
                let Some(areas) = render_pane(frame, area, &spec, theme) else {
                    return;
                };

                const API_KEY_LABEL: &str = "Enter API key: ";
                let mut first_line_spans =
                    vec![Span::styled(API_KEY_LABEL, Style::default().fg(theme.text))];
                // The real terminal caret is painted below. Avoid TextBox's
                // fallback block caret so an empty value does not put it after
                // the placeholder text.
                let tb_line = input.render_line(theme, false);
                first_line_spans.extend(tb_line.spans);

                let lines = vec![
                    Line::from(first_line_spans),
                    Line::default(),
                    Line::from(Span::styled(
                        "Key is stored via hostd auth settings.",
                        Style::default().fg(theme.dim),
                    )),
                ];

                frame.render_widget(Paragraph::new(lines), areas.content);

                let label_width = UnicodeWidthStr::width(API_KEY_LABEL);
                let origin = Position::new(
                    areas
                        .content
                        .x
                        .saturating_add(u16::try_from(label_width).unwrap_or(u16::MAX)),
                    areas.content.y,
                );
                let caret = input.caret_position(origin);
                let max_x = areas
                    .content
                    .x
                    .saturating_add(areas.content.width.saturating_sub(1));
                frame.set_cursor_position(Position::new(caret.x.min(max_x), caret.y));
            }
        }
    }
}

fn action_row(
    title: String,
    detail: String,
    provider: String,
    kind: impl Fn(String) -> AuthAction,
    authenticated: &[String],
) -> MenuRow<AuthAction> {
    let mut row = MenuRow::action(title, detail, kind(provider.clone()));
    row.is_active = authenticated.contains(&provider);
    row
}

fn group_row(title: &str, detail: &str, children: Vec<MenuRow<AuthAction>>) -> MenuRow<AuthAction> {
    MenuRow {
        title: title.to_string(),
        detail: detail.to_string(),
        value: None,
        badge: None,
        group: None,
        is_active: false,
        kind: MenuRowKind::Branch(children),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tree_marks_authenticated_providers() {
        let providers = vec!["anthropic".to_string(), "openai".to_string()];
        let rows = AuthSelector::build_menu_tree(&providers, &["anthropic".to_string()]);
        let MenuRowKind::Branch(api_key_children) = &rows[1].kind else {
            panic!("second root row must be the API-key group");
        };
        let anthropic = api_key_children
            .iter()
            .find(|r| r.title.contains("Anthropic"))
            .expect("anthropic row");
        assert!(anthropic.is_active);
        let openai = api_key_children
            .iter()
            .find(|r| r.title.contains("OpenAI"))
            .expect("openai row");
        assert!(!openai.is_active);
    }
}
