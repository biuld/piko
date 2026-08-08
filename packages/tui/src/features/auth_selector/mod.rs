use ratatui::{Frame, layout::Rect};

use piko_tui_layout::{Component, SurfacePanel};

use crate::{
    app::HitId,
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::components::{
        menu::{MenuConfirmResult, MenuRow, MenuRowKind, MenuRowLayout, MenuStack},
        text_box::TextBox,
    },
};

impl Component<HitId, Theme> for AuthSelector {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx);
    }

    fn component_regions(&self, _area: Rect) -> Vec<(Rect, HitId)> {
        Vec::new()
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

                let mut first_line_spans = vec![Span::styled(
                    "Enter API key: ",
                    Style::default().fg(theme.text),
                )];
                let tb_line = input.render_line(theme, true);
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
