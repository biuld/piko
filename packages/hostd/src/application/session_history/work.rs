use piko_protocol::{HistoryItemSummary, HistoryProvenance, HistoryWorkPage};
use piko_session_store::InspectionBundle;

use super::mapping::event_summary;

pub(super) fn work_page(
    session_id: &str,
    root_input_id: &str,
    bundle: &InspectionBundle,
    offset: usize,
    limit: usize,
) -> HistoryWorkPage {
    let input_ids = bundle
        .current
        .agent_inputs
        .values()
        .filter(|stored| stored.root_input_id.as_deref() == Some(root_input_id))
        .map(|stored| stored.input.input_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut facts = Vec::new();
    let mut diagnostics = Vec::new();
    for commit in &bundle.history.commits {
        for (index, event) in commit.events.iter().enumerate() {
            let related = event.root_input_id.as_deref() == Some(root_input_id)
                || (event.event_type.starts_with("agent_input_")
                    && event
                        .entity_id
                        .as_deref()
                        .is_some_and(|id| input_ids.contains(id)));
            if !related {
                continue;
            }
            let summary = event_summary(commit, index, event, bundle.revision, bundle);
            if summary.provenance == HistoryProvenance::Diagnostic {
                diagnostics.push(summary);
            } else {
                facts.push(summary);
            }
        }
    }
    attach_diagnostics(&mut facts, diagnostics);
    let prefix = format!("item:{root_input_id}");
    let (items, next_cursor) = super::page(facts, offset, limit, &prefix, bundle.revision);
    HistoryWorkPage {
        session_id: session_id.to_string(),
        revision: bundle.revision,
        root_input_id: root_input_id.to_string(),
        items,
        next_cursor,
    }
}

fn attach_diagnostics(facts: &mut [HistoryItemSummary], diagnostics: Vec<HistoryItemSummary>) {
    for diagnostic in diagnostics {
        if let Some(parent) = facts
            .iter_mut()
            .find(|fact| diagnostic_belongs_to(fact, &diagnostic))
        {
            parent.children.push(diagnostic);
        }
    }
}

fn diagnostic_belongs_to(fact: &HistoryItemSummary, diagnostic: &HistoryItemSummary) -> bool {
    if diagnostic.relation.tool_call_id.is_some()
        && diagnostic.relation.tool_call_id == fact.relation.tool_call_id
    {
        return true;
    }
    if diagnostic.relation.model_step_id.is_some() {
        return fact.kind.0 == "model_step"
            && fact.relation.model_step_id == diagnostic.relation.model_step_id;
    }
    fact.kind.0 == "input"
        && fact.relation.input_id == fact.relation.root_input_id
        && fact.relation.root_input_id == diagnostic.relation.root_input_id
}
