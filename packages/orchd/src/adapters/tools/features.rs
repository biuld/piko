//! F-18 managed feature gating: canonical tool → feature mapping.
//!
//! The canonical key list lives in hostd (`domain/features`); this module
//! maps piko tool definitions/names to those keys. MCP tools are identified
//! by executor kind (`"mcp"`) because their names are server-defined and
//! cannot be classified by name alone.

use std::collections::HashMap;

/// Whether a tool passes the feature gate for a resolved feature set.
pub fn feature_enabled(features: Option<&HashMap<String, bool>>, feature: Option<&str>) -> bool {
    let Some(key) = feature else {
        return true;
    };
    features
        .and_then(|map| map.get(key))
        .copied()
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_without_feature_metadata_are_ungated() {
        assert!(feature_enabled(Some(&HashMap::new()), None));
    }

    #[test]
    fn feature_gate_respects_resolved_map() {
        let mut features = HashMap::new();
        features.insert("exec".to_string(), false);
        assert!(!feature_enabled(Some(&features), Some("exec")));
    }

    #[test]
    fn absent_feature_map_keeps_everything_enabled() {
        assert!(feature_enabled(None, Some("exec")));
    }
}
