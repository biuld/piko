//! Map the published trajectory projection into list/fetch summaries.

use std::collections::HashMap;

use piko_protocol::{
    TrajectoryCostTotal, TrajectoryRecord, TrajectoryRunSummary, TrajectoryRunUsage,
};
use piko_session_store::TrajectoryRunProjection;

#[derive(Clone, Default)]
pub(super) struct DecodedSession {
    pub runs: std::collections::BTreeMap<String, DecodedRun>,
}

pub(super) type DecodedRun = TrajectoryRunProjection;

pub(super) fn summarize(
    session_id: &str,
    run_id: &str,
    run: &DecodedRun,
    dropped: &HashMap<String, u32>,
) -> TrajectoryRunSummary {
    TrajectoryRunSummary {
        session_id: session_id.to_string(),
        agent_instance_id: run.agent_instance_id.clone().unwrap_or_default(),
        run_id: run_id.to_string(),
        execution_id: run.execution_id.clone().unwrap_or_default(),
        source_turn_id: run.source_turn_id.clone(),
        started_at: run.started_at.unwrap_or_default(),
        finished_at: run.finished_at,
        terminal: run.terminal,
        step_count: run.step_count,
        tool_call_count: run.tool_call_count,
        child_run_count: run.child_run_count,
        message_count: run.messages.len() as u32,
        dropped_records: dropped.get(run_id).copied().unwrap_or(0),
        usage: run_usage(&run.records),
    }
}

/// Host-owned run-level usage rollup. Token sums add each step's
/// provider-reported input, which is cumulative over the run's conversation;
/// cost is summed per currency.
pub(super) fn run_usage(records: &[TrajectoryRecord]) -> Option<TrajectoryRunUsage> {
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut costs: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut saw_usage = false;
    for record in records {
        let TrajectoryRecord::ModelStep(step) = record else {
            continue;
        };
        let Some(usage) = step.usage.as_deref() else {
            continue;
        };
        saw_usage = true;
        input += usage.input;
        output += usage.output;
        cache_read += usage.cache_read;
        cache_write += usage.cache_write;
        for entry in &usage.cost.entries {
            *costs.entry(entry.currency.clone()).or_default() += entry.total;
        }
    }
    if !saw_usage {
        return None;
    }
    Some(TrajectoryRunUsage {
        input,
        output,
        cache_read,
        cache_write,
        cost: costs
            .into_iter()
            .map(|(currency, total)| TrajectoryCostTotal { currency, total })
            .collect(),
        cache_hit_ratio: (input > 0).then(|| cache_read as f64 / input as f64),
    })
}
