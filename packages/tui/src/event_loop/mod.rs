//! Serialized TUI cycle: input → host → tick → paint.
//!
//! One thread, one clock. Each cycle applies a bounded amount of work from
//! each ingress so streaming host lines cannot starve composer keys.

mod cycle;
mod effects;
mod input;
mod paint;

use std::{
    io::Stdout,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::event;
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    app::{AppState, command::Action, effect::Msg},
    host::HostdClient,
    input::keymap::Keymap,
    layout::PreparedFrame,
};

pub use cycle::{CycleBudget, CycleWork, should_paint};

pub fn run_bootstrap_effects(
    app: &mut AppState,
    host: &mut HostdClient,
    effects: Vec<crate::app::effect::Effect>,
) {
    run_effects(app, host, effects);
}

use cycle::CycleBudget as Budget;
use effects::run_effects;
use input::{drain_host, drain_input};
use paint::paint;

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AppState,
    host: &mut HostdClient,
    keymap: &Keymap,
    exit_after: Option<Duration>,
) -> Result<()> {
    let budget = Budget::standard();
    let started = Instant::now();
    let mut last_host_paint = Instant::now()
        .checked_sub(budget.host_paint_interval)
        .unwrap_or_else(Instant::now);
    let mut prepared = paint(terminal, app)?;

    loop {
        if should_stop(app.quit, started, exit_after) {
            return Ok(());
        }

        let work = step(app, host, keymap, &mut prepared, budget)?;
        let now = Instant::now();
        let host_due = now.duration_since(last_host_paint) >= budget.host_paint_interval;
        if should_paint(work, host_due) {
            prepared = paint(terminal, app)?;
            if work.host {
                last_host_paint = now;
            }
        }

        if should_stop(app.quit, started, exit_after) {
            return Ok(());
        }
        wait_for_next_cycle(budget);
    }
}

fn step(
    app: &mut AppState,
    host: &mut HostdClient,
    keymap: &Keymap,
    prepared: &mut PreparedFrame,
    budget: Budget,
) -> Result<CycleWork> {
    let input = drain_input(app, host, keymap, prepared, budget)?;
    let had_host = drain_host(app, host, budget);
    let tick = maybe_tick(app, host, budget);
    Ok(CycleWork {
        input,
        host: had_host,
        tick,
    })
}

fn maybe_tick(app: &mut AppState, host: &mut HostdClient, budget: Budget) -> bool {
    if app.last_tick.elapsed() <= budget.tick_interval {
        return false;
    }
    let effects = app.update(Msg::Tick);
    run_effects(app, host, effects);
    true
}

fn apply_action(app: &mut AppState, host: &mut HostdClient, action: Action) {
    let effects = app.update(Msg::Action(action));
    run_effects(app, host, effects);
}

fn should_stop(quit: bool, started: Instant, exit_after: Option<Duration>) -> bool {
    quit || exit_after.is_some_and(|limit| started.elapsed() >= limit)
}

fn wait_for_next_cycle(budget: Budget) {
    if event::poll(Duration::from_millis(0)).unwrap_or(false) {
        return;
    }
    let _ = event::poll(budget.idle_wait);
}
