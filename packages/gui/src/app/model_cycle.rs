//! Pure helpers for cycling default model / thinking from Core projection.

use piko_client_core::ClientIntent;
use piko_protocol::{ProviderInfo, ThinkingLevel};

const THINKING_CYCLE: &[ThinkingLevel] = &[
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

/// Next model in the flattened catalog, wrapping around.
pub fn next_model_intent(
    providers: &[ProviderInfo],
    current_provider: Option<&str>,
    current_model: Option<&str>,
) -> Option<ClientIntent> {
    let models = catalog_models(providers);
    if models.is_empty() {
        return None;
    }

    let current_ix = models.iter().position(|(p, id, _)| {
        Some(p.as_str()) == current_provider && Some(id.as_str()) == current_model
    });
    let next_ix = match current_ix {
        Some(ix) => (ix + 1) % models.len(),
        None => 0,
    };
    let (provider, model_id, _) = &models[next_ix];
    Some(ClientIntent::SetModel {
        provider: provider.clone(),
        model_id: model_id.clone(),
    })
}

/// Next thinking level in the fixed cycle.
pub fn next_thinking_intent(current: Option<&str>) -> ClientIntent {
    let ix = current
        .and_then(|s| THINKING_CYCLE.iter().position(|l| l.as_str() == s))
        .unwrap_or(0);
    let next = THINKING_CYCLE[(ix + 1) % THINKING_CYCLE.len()].clone();
    ClientIntent::SetThinkingLevel { level: next }
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
    fn cycles_model() {
        let providers = vec![provider("p", true, &["m1", "m2"])];
        let intent = next_model_intent(&providers, Some("p"), Some("m1")).unwrap();
        match intent {
            ClientIntent::SetModel { provider, model_id } => {
                assert_eq!(provider, "p");
                assert_eq!(model_id, "m2");
            }
            _ => panic!("expected SetModel"),
        }
    }

    #[test]
    fn cycles_thinking() {
        match next_thinking_intent(Some("off")) {
            ClientIntent::SetThinkingLevel { level } => {
                assert_eq!(level, ThinkingLevel::Minimal);
            }
            _ => panic!("expected SetThinkingLevel"),
        }
    }
}
