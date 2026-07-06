use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::{
        hierarchical_menu::{HierarchicalMenu, MenuConfirmResult, MenuNode},
        text_box::TextBox,
    },
};

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
    pub menu: HierarchicalMenu<AuthAction>,
    pub filter: String,
}

impl AuthSelector {
    pub fn new(available_providers: &[String]) -> Self {
        let root = Self::build_menu_tree(available_providers);
        Self {
            state: AuthSelectorState::Menu,
            menu: HierarchicalMenu::new(root),
            filter: String::new(),
        }
    }

    pub fn build_menu_tree(available_providers: &[String]) -> MenuNode<AuthAction> {
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
                MenuNode::Action {
                    title,
                    detail,
                    action: AuthAction::StartOAuth { provider: p },
                }
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
                MenuNode::Action {
                    title,
                    detail,
                    action: AuthAction::StartApiKey { provider: p },
                }
            })
            .collect();

        MenuNode::Group {
            title: "authentication".to_string(),
            detail: "Configure provider authentication".to_string(),
            children: vec![
                MenuNode::Group {
                    title: "Use a subscription (OAuth)".to_string(),
                    detail: "Log in using a web browser subscription".to_string(),
                    children: oauth_children,
                },
                MenuNode::Group {
                    title: "Use an API Key".to_string(),
                    detail: "Manually configure an API key".to_string(),
                    children: api_key_children,
                },
            ],
        }
    }

    pub fn reset(&mut self, available_providers: &[String]) {
        self.state = AuthSelectorState::Menu;
        self.filter.clear();
        let root = Self::build_menu_tree(available_providers);
        self.menu.open(root);
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.menu
            .stack
            .last()
            .map(|frame| frame.list.items.len())
            .unwrap_or(0)
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
                MenuConfirmResult::Action(AuthAction::StartOAuth { provider }, _) => {
                    AuthConfirmResult::StartOAuth { provider }
                }
                MenuConfirmResult::Action(AuthAction::StartApiKey { provider }, _) => {
                    self.state = AuthSelectorState::ApiKeyInput {
                        provider,
                        input: TextBox::new()
                            .with_mask('•')
                            .with_placeholder("Paste API key here..."),
                    };
                    self.filter.clear();
                    AuthConfirmResult::StartApiKeyInput
                }
                MenuConfirmResult::SubMenuPushed | MenuConfirmResult::None => {
                    AuthConfirmResult::None
                }
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
                self.menu
                    .render(frame, area, &self.filter, |_action| false, theme);
            }
            AuthSelectorState::ApiKeyInput { provider, input } => {
                use ratatui::style::Style;
                use ratatui::text::{Line, Span};
                use ratatui::widgets::{Block, Borders, Paragraph};

                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_accent))
                    .title(format!(" Configure {} API Key ", provider));
                frame.render_widget(block, area);

                let inner_area = Rect::new(
                    area.x + 2,
                    area.y + 2,
                    area.width.saturating_sub(4),
                    area.height.saturating_sub(4),
                );

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
                        "Press Enter to save · Esc to go back",
                        Style::default().fg(theme.muted),
                    )),
                ];

                frame.render_widget(Paragraph::new(lines), inner_area);
            }
        }
    }
}
