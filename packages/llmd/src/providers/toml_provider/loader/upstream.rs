use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::UpstreamToolToml;
use crate::capabilities::UpstreamToolKind;
use crate::modeling::UpstreamToolSupport;
use crate::tools::UpstreamApprovalPolicy;

fn parse_kind(value: &str, owner: &str) -> Result<UpstreamToolKind, String> {
    UpstreamToolKind::new(value)
        .map_err(|error| format!("Invalid upstream tool kind for {owner}: {error}"))
}

pub(super) fn parse_upstream_kind_set(
    values: Option<&Vec<String>>,
    owner: &str,
) -> Result<Option<BTreeSet<UpstreamToolKind>>, String> {
    values
        .map(|values| {
            values
                .iter()
                .map(|value| parse_kind(value, owner))
                .collect()
        })
        .transpose()
}

pub(super) fn build_upstream_tools(
    tools: &HashMap<String, UpstreamToolToml>,
) -> Result<BTreeMap<UpstreamToolKind, UpstreamToolSupport>, String> {
    let mut activity_owners = HashSet::new();
    tools
        .iter()
        .map(|(kind, tool)| {
            let kind = parse_kind(kind, "tool catalog")?;
            let name = tool.name.clone().unwrap_or_else(|| kind.as_str().into());
            if name.trim().is_empty() || name.len() > 128 {
                return Err(format!("Invalid model-visible name for upstream tool {kind}"));
            }
            let definition_type = tool
                .definition
                .as_object()
                .and_then(|definition| definition.get("type"))
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!("Upstream tool {kind} definition must be an object with a string type")
                })?;
            if tool.choice.as_ref().is_some_and(|choice| !choice.is_object()) {
                return Err(format!("Upstream tool {kind} choice must be an object"));
            }
            for activity_type in &tool.activity_types {
                if activity_type.is_empty() || !activity_owners.insert(activity_type.clone()) {
                    return Err(format!(
                        "Upstream activity type must be non-empty and uniquely owned: {activity_type}"
                    ));
                }
            }
            let approval = match tool.approval.as_deref().unwrap_or("never") {
                "never" => UpstreamApprovalPolicy::Never,
                "on_request" => UpstreamApprovalPolicy::OnRequest,
                "always" => UpstreamApprovalPolicy::Always,
                value => return Err(format!("Unknown upstream approval policy: {value}")),
            };
            Ok((
                kind.clone(),
                UpstreamToolSupport {
                    kind,
                    name,
                    approval,
                    wire_definition: tool.definition.clone(),
                    wire_choice: tool.choice.clone().unwrap_or_else(|| {
                        serde_json::json!({"type": definition_type})
                    }),
                    activity_types: tool.activity_types.clone(),
                },
            ))
        })
        .collect()
}

pub(super) fn validate_kind_references<'a>(
    owner: &str,
    references: impl IntoIterator<Item = &'a UpstreamToolKind>,
    definitions: &BTreeSet<UpstreamToolKind>,
) -> Result<(), String> {
    for kind in references {
        if !definitions.contains(kind) {
            return Err(format!(
                "{owner} references undefined upstream tool kind {kind}"
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_effective_catalog(
    owner: &str,
    catalog: &BTreeMap<UpstreamToolKind, UpstreamToolSupport>,
) -> Result<(), String> {
    let mut activity_owners = HashMap::<&str, &UpstreamToolKind>::new();
    for (kind, tool) in catalog {
        for activity_type in &tool.activity_types {
            if let Some(existing) = activity_owners.insert(activity_type, kind) {
                return Err(format!(
                    "{owner} maps upstream activity type {activity_type} to both {existing} and {kind}"
                ));
            }
        }
    }
    Ok(())
}
