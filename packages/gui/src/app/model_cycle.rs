//! Model / thinking catalog helpers for the Command Palette.

use piko_protocol::{ProviderInfo, ThinkingLevel};

/// Fixed thinking levels offered in the Palette submenu (and formerly cycle order).
pub const THINKING_LEVELS: &[ThinkingLevel] = &[
    ThinkingLevel::Off,
    ThinkingLevel::Minimal,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
    ThinkingLevel::XHigh,
];

/// Flatten catalog into `(provider, model_id, display_name)` preferring authed providers.
pub fn catalog_models(providers: &[ProviderInfo]) -> Vec<(String, String, String)> {
    let authed: Vec<_> = providers.iter().filter(|p| p.has_auth).collect();
    let source: Vec<&ProviderInfo> = if authed.is_empty() {
        providers.iter().collect()
    } else {
        authed
    };

    source
        .into_iter()
        .flat_map(|p| {
            p.models
                .iter()
                .map(move |m| (p.provider.clone(), m.id.clone(), m.name.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::ModelSummary;

    fn provider(name: &str, has_auth: bool, models: &[&str]) -> ProviderInfo {
        ProviderInfo {
            provider: name.into(),
            has_auth,
            models: models
                .iter()
                .map(|id| ModelSummary {
                    id: (*id).into(),
                    name: (*id).into(),
                    reasoning: false,
                    input: vec![],
                    context_window: 0,
                    max_tokens: 0,
                    thinking_level_map: None,
                })
                .collect(),
        }
    }

    #[test]
    fn prefers_authed_catalog() {
        let providers = vec![
            provider("plain", false, &["a"]),
            provider("authed", true, &["b", "c"]),
        ];
        let models = catalog_models(&providers);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].0, "authed");
    }

    #[test]
    fn thinking_levels_are_stable() {
        assert_eq!(THINKING_LEVELS[0], ThinkingLevel::Off);
        assert_eq!(THINKING_LEVELS.last(), Some(&ThinkingLevel::XHigh));
    }
}
