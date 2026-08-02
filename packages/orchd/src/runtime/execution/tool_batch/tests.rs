//! F-06 tool batch dispatch acceptance tests.

use std::sync::Arc;
use std::time::Duration;

use piko_protocol::Message;
use piko_protocol::execution::{CancelExecutionRequest, CancelReason};
use piko_protocol::tools::ToolSetPolicy;

use super::fixtures::*;
use crate::domain::tools::definition::ToolExecutionMode;

fn tool_kind_sequence(transcript: &[Message]) -> Vec<&'static str> {
    transcript
        .iter()
        .filter_map(|message| match message {
            Message::ToolCall { .. } => Some("tc"),
            Message::ToolResult { .. } => Some("tr"),
            _ => None,
        })
        .collect()
}

fn tool_results(transcript: &[Message]) -> Vec<&Message> {
    transcript
        .iter()
        .filter(|message| matches!(message, Message::ToolResult { .. }))
        .collect()
}

fn tool_result_text(message: &Message) -> String {
    match message {
        Message::ToolResult { content, .. } => match &content[0] {
            crate::domain::transcript::ContentBlock::Text { text } => text.clone(),
            _ => panic!("unexpected content block"),
        },
        _ => panic!("not a tool result"),
    }
}

#[tokio::test]
async fn parallel_calls_overlap_and_commit_in_call_order() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "par_a"), ("call-1", "par_b")]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness
        .provider
        .set_delay("par_a", Duration::from_millis(100));
    harness
        .provider
        .set_delay("par_b", Duration::from_millis(100));

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-parallel", tools, routes).await;

    assert!(matches!(
        terminal.outcome,
        piko_protocol::execution::ExecutionOutcome::Succeeded { .. }
    ));
    assert_eq!(
        harness.provider.max_concurrent(),
        2,
        "parallel calls must overlap"
    );
    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tc", "tr", "tr"]
    );
    let results = tool_results(&terminal.transcript);
    assert_eq!(tool_result_text(results[0]), "result-par_a");
    assert_eq!(tool_result_text(results[1]), "result-par_b");
}

#[tokio::test]
async fn sequential_call_in_mixed_batch_overlaps_nothing() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[
        ("call-0", "par_a"),
        ("call-1", "par_b"),
        ("call-2", "seq_c"),
    ]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    for tool in ["par_a", "par_b", "seq_c"] {
        harness.provider.set_delay(tool, Duration::from_millis(80));
    }

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-mixed", tools, routes).await;

    assert_eq!(
        harness.provider.max_concurrent(),
        2,
        "parallel pair overlaps"
    );
    let events = harness.provider.events();
    let phase_at = |tool: &str, phase: TimingPhase| {
        events
            .iter()
            .find(|event| event.tool == tool && event.phase == phase)
            .expect("event present")
            .at
    };
    assert!(
        phase_at("seq_c", TimingPhase::Started) >= phase_at("par_a", TimingPhase::Finished)
            && phase_at("seq_c", TimingPhase::Started) >= phase_at("par_b", TimingPhase::Finished),
        "sequential call must not overlap parallel calls"
    );
    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tc", "tr", "tr", "tc", "tr"]
    );
}

#[tokio::test]
async fn concurrency_cap_of_one_serializes_parallel_calls() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "par_a"), ("call-1", "par_b")]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(
        gateway.clone(),
        Some(ToolSetPolicy {
            defaults: None,
            allow_parallel: None,
            max_concurrent_calls: Some(1),
        }),
    )
    .await;
    for tool in ["par_a", "par_b"] {
        harness.provider.set_delay(tool, Duration::from_millis(50));
    }

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-cap", tools, routes).await;

    assert_eq!(
        harness.provider.max_concurrent(),
        1,
        "cap of 1 must serialize parallel calls"
    );
    let results = tool_results(&terminal.transcript);
    assert_eq!(tool_result_text(results[0]), "result-par_a");
    assert_eq!(tool_result_text(results[1]), "result-par_b");
}

#[tokio::test]
async fn results_commit_in_call_order_when_completion_order_differs() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "par_a"), ("call-1", "par_b")]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness
        .provider
        .set_delay("par_a", Duration::from_millis(150));
    harness
        .provider
        .set_delay("par_b", Duration::from_millis(20));

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-order", tools, routes).await;

    let events = harness.provider.events();
    let finished_order: Vec<&str> = events
        .iter()
        .filter(|event| event.phase == TimingPhase::Finished)
        .map(|event| event.tool.as_str())
        .collect();
    assert_eq!(
        finished_order,
        ["par_b", "par_a"],
        "par_b must finish first"
    );

    let results = tool_results(&terminal.transcript);
    assert_eq!(tool_result_text(results[0]), "result-par_a");
    assert_eq!(tool_result_text(results[1]), "result-par_b");
}

#[tokio::test]
async fn cancellation_mid_batch_commits_aborted_results_for_every_call() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "par_a"), ("call-1", "par_b")]));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness.provider.enable_hold();

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let prepared = harness
        .runtime
        .prepare_execution(
            batch_request("exec-cancel", tools),
            routes,
            tracing::Span::none(),
        )
        .await
        .unwrap();
    prepared.activate().await;

    harness.provider.wait_entered(2).await;
    harness
        .runtime
        .request_cancel(CancelExecutionRequest {
            request_id: "cancel-batch".into(),
            session_id: "session-batch".into(),
            execution_id: "exec-cancel".into(),
            reason: CancelReason::UserRequested,
        })
        .await
        .unwrap();

    let terminal = harness
        .runtime
        .wait_terminal_state("session-batch", "exec-cancel")
        .await
        .unwrap();

    assert!(matches!(
        terminal.outcome,
        piko_protocol::execution::ExecutionOutcome::Cancelled { .. }
    ));
    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tc", "tr", "tr"],
        "every in-flight call must commit an aborted result"
    );
    for result in tool_results(&terminal.transcript) {
        match result {
            Message::ToolResult {
                is_error: Some(true),
                details: Some(details),
                ..
            } => {
                assert_eq!(details["code"], "aborted");
            }
            _ => panic!("cancelled calls must commit bounded error results"),
        }
        assert_eq!(tool_result_text(result), "Task cancelled");
    }
    assert_eq!(harness.provider.execution_count("par_a"), 1);
    assert_eq!(harness.provider.execution_count("par_b"), 1);
    assert_eq!(gateway.call_count(), 1, "no further model call may start");

    let durable_kinds: Vec<&str> = harness
        .commits
        .messages()
        .iter()
        .filter_map(|commit| match &commit.message {
            Message::ToolCall { .. } => Some("tc"),
            Message::ToolResult { .. } => Some("tr"),
            _ => None,
        })
        .collect();
    assert_eq!(
        durable_kinds,
        ["tc", "tc", "tr", "tr"],
        "aborted results must be durably committed"
    );
}

#[tokio::test]
async fn cancel_during_sequential_call_does_not_start_pending_parallel_calls() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "seq_c"), ("call-1", "par_a")]));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness.provider.enable_hold();

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let prepared = harness
        .runtime
        .prepare_execution(
            batch_request("exec-cancel-seq", tools),
            routes,
            tracing::Span::none(),
        )
        .await
        .unwrap();
    prepared.activate().await;

    harness.provider.wait_entered(1).await;
    harness
        .runtime
        .request_cancel(CancelExecutionRequest {
            request_id: "cancel-seq".into(),
            session_id: "session-batch".into(),
            execution_id: "exec-cancel-seq".into(),
            reason: CancelReason::UserRequested,
        })
        .await
        .unwrap();

    let terminal = harness
        .runtime
        .wait_terminal_state("session-batch", "exec-cancel-seq")
        .await
        .unwrap();

    assert!(matches!(
        terminal.outcome,
        piko_protocol::execution::ExecutionOutcome::Cancelled { .. }
    ));
    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tr", "tc", "tr"],
        "never-started calls must still receive committed aborted results"
    );
    assert_eq!(harness.provider.execution_count("seq_c"), 1);
    assert_eq!(
        harness.provider.execution_count("par_a"),
        0,
        "pending parallel call must not start after cancellation"
    );
    for result in tool_results(&terminal.transcript) {
        assert_eq!(tool_result_text(result), "Task cancelled");
    }
    assert_eq!(gateway.call_count(), 1);
}

#[tokio::test]
async fn unknown_tool_commits_bounded_error_and_does_not_block_batch() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "par_a"), ("call-1", "ghost")]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness
        .provider
        .set_delay("par_a", Duration::from_millis(50));

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-unknown", tools, routes).await;

    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tr", "tc", "tr"]
    );
    let results = tool_results(&terminal.transcript);
    assert_eq!(tool_result_text(results[0]), "result-par_a");
    assert!(
        tool_result_text(results[1]).contains("No route for tool \"ghost\""),
        "unknown tool must keep its bounded error shape"
    );
}

#[tokio::test]
async fn all_sequential_step_keeps_per_call_transcript_shape() {
    let gateway = Arc::new(ToolCallingGateway::new());
    gateway.push_step(tool_use_step(&[("call-0", "seq_c"), ("call-1", "seq_d")]));
    gateway.push_step(text_step("finished"));
    let harness = tool_batch_harness(gateway.clone(), None).await;
    harness
        .provider
        .set_delay("seq_c", Duration::from_millis(40));
    harness
        .provider
        .set_delay("seq_d", Duration::from_millis(40));

    let (tools, routes) = discover_batch_routes(&harness.runtime).await;
    let terminal = run_batch(&harness.runtime, "exec-sequential", tools, routes).await;

    assert_eq!(harness.provider.max_concurrent(), 1);
    assert_eq!(
        tool_kind_sequence(&terminal.transcript),
        ["tc", "tr", "tc", "tr"],
        "all-sequential steps keep the per-call ToolCall/result loop shape"
    );
    let results = tool_results(&terminal.transcript);
    assert_eq!(tool_result_text(results[0]), "result-seq_c");
    assert_eq!(tool_result_text(results[1]), "result-seq_d");
}

#[tokio::test]
async fn routes_carry_resolved_mode_and_set_cap() {
    let gateway = Arc::new(ToolCallingGateway::new());
    let harness = tool_batch_harness(
        gateway,
        Some(ToolSetPolicy {
            defaults: None,
            allow_parallel: Some(true),
            max_concurrent_calls: Some(3),
        }),
    )
    .await;
    let (_tools, routes) = discover_batch_routes(&harness.runtime).await;
    assert_eq!(routes["par_a"].execution_mode, ToolExecutionMode::Parallel);
    assert_eq!(
        routes["seq_c"].execution_mode,
        ToolExecutionMode::Sequential
    );
    assert_eq!(routes["par_a"].max_concurrent_calls, Some(3));
    assert_eq!(routes["seq_c"].max_concurrent_calls, Some(3));
}
