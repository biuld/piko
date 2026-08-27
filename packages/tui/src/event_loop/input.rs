use std::time::Instant;

use anyhow::{Context, Result};
use crossterm::event::{self, Event as CrosstermEvent};

use crate::{
    app::{
        AppState,
        command::{Action, TimelineAction},
    },
    input::{batch::TimelineScrollBatch, focus::InputRouter},
    layout::PreparedFrame,
    terminal::InputNormalizer,
};

use super::{CycleBudget, apply_action, effects::run_effects};
use crate::host::HostdClient;

pub(super) fn drain_input(
    app: &mut AppState,
    host: &mut HostdClient,
    normalizer: &InputNormalizer,
    prepared: &mut PreparedFrame,
    budget: CycleBudget,
) -> Result<bool> {
    if !event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
        return Ok(false);
    }

    let batch_started = Instant::now();
    let mut processed = 0usize;
    let mut timeline_scroll = TimelineScrollBatch::default();
    let mut state_changed = false;
    loop {
        let mut end_batch = false;
        match event::read().context("read terminal event")? {
            event @ (CrosstermEvent::Key(_)
            | CrosstermEvent::Paste(_)
            | CrosstermEvent::Mouse(_)
            | CrosstermEvent::Resize(_, _)
            | CrosstermEvent::FocusGained
            | CrosstermEvent::FocusLost) => {
                if let Some(input) = normalizer.normalize(event) {
                    match input {
                        crate::terminal::NormalizedInput::Key { .. } => {
                            flush_timeline_scroll(app, host, &mut timeline_scroll);
                            if let Some(action) =
                                InputRouter::route_input(app, &app.binding_registry, input)
                            {
                                apply_action(app, host, action);
                                end_batch = true;
                            }
                        }
                        crate::terminal::NormalizedInput::Paste(text) => {
                            flush_timeline_scroll(app, host, &mut timeline_scroll);
                            if let Some(action) = InputRouter::route_input(
                                app,
                                &app.binding_registry,
                                normalizer.normalize_paste(text),
                            ) {
                                apply_action(app, host, action);
                            }
                            end_batch = true;
                        }
                        crate::terminal::NormalizedInput::Pointer(pointer) => {
                            prepared.refresh_timeline(app);
                            app.pointer_position = Some((pointer.column, pointer.row));
                            for action in
                                crate::input::pointer::route_normalized_pointer_with_hitmap(
                                    app, prepared, pointer,
                                )
                            {
                                match action {
                                    Action::Timeline(
                                        action @ (TimelineAction::ScrollUp(_)
                                        | TimelineAction::ScrollDown(_)),
                                    ) => {
                                        if let Some(flushed) = timeline_scroll.push(action) {
                                            apply_action(app, host, flushed.into());
                                            state_changed = true;
                                        }
                                    }
                                    action => {
                                        flush_timeline_scroll(app, host, &mut timeline_scroll);
                                        apply_action(app, host, action);
                                        end_batch = true;
                                    }
                                }
                            }
                        }
                        crate::terminal::NormalizedInput::Resize { .. }
                        | crate::terminal::NormalizedInput::FocusGained
                        | crate::terminal::NormalizedInput::FocusLost => {
                            end_batch = true;
                        }
                    }
                }
            }
        }
        processed = processed.saturating_add(1);
        state_changed |= end_batch;

        if app.quit
            || end_batch
            || processed >= budget.max_input_events
            || batch_started.elapsed() >= budget.input_time
        {
            break;
        }
        if !event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
            break;
        }
    }
    if let Some(action) = timeline_scroll.take() {
        apply_action(app, host, action.into());
        state_changed = true;
    }
    // Hover is re-derived from the last pointer position after any viewport
    // change in this batch (wheel, keyboard scroll, jump_latest).
    prepared.refresh_timeline(app);
    app.reconcile_hover_after_viewport_change(prepared);
    Ok(state_changed)
}

fn flush_timeline_scroll(
    app: &mut AppState,
    host: &mut HostdClient,
    batch: &mut TimelineScrollBatch,
) {
    if let Some(action) = batch.take() {
        apply_action(app, host, action.into());
    }
}

pub(super) fn drain_host(app: &mut AppState, host: &mut HostdClient, budget: CycleBudget) -> bool {
    let lines = host.drain_up_to(budget.max_host_lines);
    if lines.is_empty() {
        return false;
    }
    app.begin_host_batch();
    for line in lines {
        let effects = app.update(crate::app::effect::Msg::HostLine(line));
        run_effects(app, host, effects);
    }
    app.end_host_batch();
    true
}
