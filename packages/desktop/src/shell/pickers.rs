//! Pure model/thinking picker trees. Mapped to Island menus at the GPUI boundary.

use piko_client_core::state::ModelState;
use piko_protocol::{ModelSummary, ThinkingLevel};

use super::tabs::thinking_chrome_label;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerPayload {
    SetModel { provider: String, model_id: String },
    SetThinking(ThinkingLevel),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerEntry {
    Header {
        label: String,
    },
    Separator,
    Action {
        label: String,
        selected: bool,
        enabled: bool,
        payload: PickerPayload,
    },
}

pub fn thinking_entries(current: Option<&str>) -> Vec<PickerEntry> {
    [
        ThinkingLevel::Off,
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
        ThinkingLevel::Max,
    ]
    .into_iter()
    .map(|level| {
        let selected = current == Some(level.as_str());
        PickerEntry::Action {
            label: thinking_menu_label(&level),
            selected,
            enabled: true,
            payload: PickerPayload::SetThinking(level),
        }
    })
    .collect()
}

fn thinking_menu_label(level: &ThinkingLevel) -> String {
    let title = thinking_chrome_label(level.as_str());
    match level {
        ThinkingLevel::Off => format!("{title} — no extra reasoning"),
        ThinkingLevel::Minimal => format!("{title} — shortest"),
        ThinkingLevel::Medium => format!("{title} — default"),
        _ => title,
    }
}

pub fn model_entries(state: &ModelState) -> Vec<PickerEntry> {
    let total: usize = state.providers.iter().map(|p| p.models.len()).sum();
    if total == 0 {
        return vec![PickerEntry::Action {
            label: "No models listed".into(),
            selected: false,
            enabled: false,
            payload: PickerPayload::None,
        }];
    }
    let grouped = state.providers.len() >= 2;
    let mut entries = Vec::new();
    for (index, provider) in state.providers.iter().enumerate() {
        if grouped {
            if index > 0 {
                entries.push(PickerEntry::Separator);
            }
            entries.push(PickerEntry::Header {
                label: provider.provider.clone(),
            });
        }
        entries.extend(
            provider
                .models
                .iter()
                .map(|model| model_action(state, &provider.provider, model)),
        );
    }
    entries
}

fn model_action(state: &ModelState, provider: &str, model: &ModelSummary) -> PickerEntry {
    PickerEntry::Action {
        label: model.id.clone(),
        selected: model_row_matches(
            state.model_id.as_deref(),
            state.provider.as_deref(),
            provider,
            model,
        ),
        enabled: true,
        payload: PickerPayload::SetModel {
            provider: provider.to_string(),
            model_id: model.id.clone(),
        },
    }
}

pub fn model_row_matches(
    current_id: Option<&str>,
    current_provider: Option<&str>,
    provider: &str,
    model: &ModelSummary,
) -> bool {
    let Some(id) = current_id else {
        return false;
    };
    let full = format!("{provider}/{}", model.id);
    let provider_matches = current_provider.is_none_or(|p| p == provider);
    id == full || (provider_matches && id == model.id) || id == model.name
}

#[cfg(test)]
mod tests {
    use super::*;
    use piko_protocol::{ProviderInfo, ThinkingLevel};

    fn summary(id: &str, name: &str, window: u64) -> ModelSummary {
        ModelSummary {
            id: id.into(),
            name: name.into(),
            reasoning: false,
            input: Vec::new(),
            context_window: window,
            max_tokens: 0,
            reasoning_efforts: Vec::new(),
            output: Vec::new(),
            tool_execution_loci: Vec::new(),
            parallel_tool_calls: false,
            structured_output: false,
            delivery_modes: Vec::new(),
        }
    }

    fn provider(name: &str, models: Vec<ModelSummary>) -> ProviderInfo {
        ProviderInfo {
            provider: name.into(),
            models,
            has_auth: true,
            auth_methods: Vec::new(),
        }
    }

    #[test]
    fn thinking_lists_seven_with_captions_and_checkmark() {
        let entries = thinking_entries(Some("high"));
        assert_eq!(entries.len(), 7);
        match &entries[4] {
            PickerEntry::Action {
                label,
                selected,
                payload,
                ..
            } => {
                assert_eq!(label, "High");
                assert!(selected);
                assert_eq!(payload, &PickerPayload::SetThinking(ThinkingLevel::High));
            }
            other => panic!("{other:?}"),
        }
        match &entries[0] {
            PickerEntry::Action { label, .. } => {
                assert_eq!(label, "Off — no extra reasoning");
            }
            other => panic!("{other:?}"),
        }
        match &entries[1] {
            PickerEntry::Action { label, .. } => assert_eq!(label, "Minimal — shortest"),
            other => panic!("{other:?}"),
        }
        match &entries[3] {
            PickerEntry::Action { label, .. } => assert_eq!(label, "Medium — default"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn one_provider_is_a_flat_list() {
        let state = ModelState {
            model_id: Some("m1".into()),
            provider: Some("acme".into()),
            providers: vec![provider("acme", vec![summary("m1", "One", 0)])],
            ..ModelState::default()
        };
        let entries = model_entries(&state);
        assert!(matches!(
            &entries[..],
            [PickerEntry::Action {
                label,
                selected: true,
                ..
            }] if label == "m1"
        ));
        assert!(
            !entries
                .iter()
                .any(|e| matches!(e, PickerEntry::Header { .. }))
        );
    }

    #[test]
    fn two_providers_use_headers() {
        let state = ModelState {
            providers: vec![
                provider("a", vec![summary("1", "1", 1), summary("2", "2", 1)]),
                provider("b", vec![summary("3", "3", 1), summary("4", "4", 1)]),
            ],
            ..ModelState::default()
        };
        let entries = model_entries(&state);
        assert!(matches!(entries[0], PickerEntry::Header { .. }));
        assert!(entries.iter().any(|e| matches!(e, PickerEntry::Separator)));
    }

    #[test]
    fn large_catalogs_stay_a_single_grouped_list() {
        let models = (0..8)
            .map(|i| summary(&format!("m{i}"), &format!("n{i}"), 1))
            .collect::<Vec<_>>();
        let state = ModelState {
            providers: vec![
                provider("a", models.clone()),
                provider("b", models.clone()),
                provider("c", models),
            ],
            ..ModelState::default()
        };
        let entries = model_entries(&state);
        assert!(entries.iter().any(|e| matches!(
            e,
            PickerEntry::Header { label } if label == "a"
        )));
        assert_eq!(
            entries
                .iter()
                .filter(|e| matches!(e, PickerEntry::Action { .. }))
                .count(),
            24
        );
    }

    #[test]
    fn empty_catalog_is_a_disabled_placeholder() {
        let entries = model_entries(&ModelState::default());
        assert_eq!(
            entries,
            vec![PickerEntry::Action {
                label: "No models listed".into(),
                selected: false,
                enabled: false,
                payload: PickerPayload::None,
            }]
        );
    }

    #[test]
    fn checkmark_ignores_zero_context_window() {
        let model = summary("flash", "Flash", 0);
        assert!(model_row_matches(
            Some("acme/flash"),
            Some("acme"),
            "acme",
            &model
        ));
        assert!(model_row_matches(
            Some("flash"),
            Some("acme"),
            "acme",
            &model
        ));
        assert!(model_row_matches(Some("Flash"), None, "acme", &model));
        assert!(!model_row_matches(
            Some("other"),
            Some("acme"),
            "acme",
            &model
        ));
    }
}
