//! F-18 managed feature gating: canonical feature catalog and resolution.
//!
//! A `[features]` settings section declares named tool-family features.
//! `enabled` sets explicit values per key (merged per key across layers);
//! `managed` pins features to a fixed value that is the final authority
//! over `enabled` in every layer. Resolution is pure and unit-tested here;
//! hostd passes the resolved map to orchd, which applies it to the tool
//! catalog. With no `[features]` section every feature resolves enabled
//! (today's behavior).

use std::collections::HashMap;

use crate::domain::config::FeaturesSettings;

/// Canonical feature keys mapped to piko tool families. The same keys are
/// consumed by orchd's catalog filter (`feature_for_tool`).
pub const FEATURE_KEYS: &[&str] = &[
    "workspace",
    "bash",
    "process",
    "environment",
    "context",
    "todo",
    "multi-agent",
    "user-interaction",
    "mcp",
];

/// Resolved managed features for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFeatures {
    /// Resolved boolean per canonical key; every key is present.
    pub enabled: HashMap<String, bool>,
    /// Startup warnings surfaced for unknown keys and pin conflicts.
    pub warnings: Vec<String>,
}

impl Default for ResolvedFeatures {
    fn default() -> Self {
        Self {
            enabled: FEATURE_KEYS
                .iter()
                .map(|key| ((*key).to_string(), true))
                .collect(),
            warnings: Vec::new(),
        }
    }
}

/// Resolve the active feature set from merged settings.
///
/// - No `[features]` section: every canonical feature enabled.
/// - `enabled` map: per-key explicit values; unknown keys warn and are
///   ignored (they can never disable anything).
/// - `managed` map: pins are the final authority; a merged `enabled` value
///   that contradicts a pin logs a warning and the pin wins (deterministic,
///   fail-closed); unknown pins warn and are ignored.
pub fn resolve_features(settings: Option<&FeaturesSettings>) -> ResolvedFeatures {
    let mut resolved = ResolvedFeatures::default();
    let mut warnings = Vec::new();

    let Some(settings) = settings else {
        return resolved;
    };

    for (key, enabled) in &settings.enabled {
        if FEATURE_KEYS.contains(&key.as_str()) {
            resolved.enabled.insert(key.clone(), *enabled);
        } else {
            warnings.push(format!(
                "ignoring unknown feature '{}' in [features] enabled",
                key
            ));
        }
    }

    for (key, pinned) in &settings.managed {
        if !FEATURE_KEYS.contains(&key.as_str()) {
            warnings.push(format!(
                "ignoring unknown managed feature '{}' in [features] managed",
                key
            ));
            continue;
        }
        if let Some(explicit) = settings.enabled.get(key)
            && explicit != pinned
        {
            warnings.push(format!(
                "feature '{}' is pinned to {}; ignoring conflicting [features] enabled value",
                key, pinned
            ));
        }
        resolved.enabled.insert(key.clone(), *pinned);
    }

    resolved.warnings = warnings;
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(enabled: &[(&str, bool)], managed: &[(&str, bool)]) -> FeaturesSettings {
        FeaturesSettings {
            enabled: enabled
                .iter()
                .map(|(key, value)| ((*key).to_string(), *value))
                .collect(),
            managed: managed
                .iter()
                .map(|(key, value)| ((*key).to_string(), *value))
                .collect(),
        }
    }

    #[test]
    fn no_features_section_resolves_everything_enabled() {
        let resolved = resolve_features(None);
        assert!(resolved.enabled.values().all(|enabled| *enabled));
        assert_eq!(resolved.enabled.len(), FEATURE_KEYS.len());
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn explicit_disable_turns_feature_off() {
        let settings = features(&[("process", false)], &[]);
        let resolved = resolve_features(Some(&settings));
        assert!(!resolved.enabled["process"]);
        assert!(resolved.enabled["bash"]);
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn managed_pin_wins_over_enabled_in_same_layer() {
        let settings = features(&[("process", true)], &[("process", false)]);
        let resolved = resolve_features(Some(&settings));
        assert!(!resolved.enabled["process"]);
        assert_eq!(resolved.warnings.len(), 1);
        assert!(resolved.warnings[0].contains("pinned to false"));
    }

    #[test]
    fn managed_pin_matching_enabled_is_silent() {
        let settings = features(&[("mcp", false)], &[("mcp", false)]);
        let resolved = resolve_features(Some(&settings));
        assert!(!resolved.enabled["mcp"]);
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn unknown_keys_warn_and_are_ignored() {
        let settings = features(&[("proces", false)], &[("bogus", true)]);
        let resolved = resolve_features(Some(&settings));
        // Unknown keys disable nothing; canonical keys stay enabled.
        assert!(resolved.enabled.values().all(|enabled| *enabled));
        assert_eq!(resolved.warnings.len(), 2);
    }
}
