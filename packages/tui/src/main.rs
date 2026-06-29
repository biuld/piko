mod action;
mod app;
mod cli;
mod host;
mod input;
mod notification;
mod render;
mod surfaces;
mod text;
mod tui;

use std::{
    env,
    io::Stdout,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use app::{AppState, InitialOptions};
use cli::CliArgs;
use crossterm::event::{self, Event as CrosstermEvent};
use host::HostdClient;
use input::keymap::Keymap;
use ratatui::{Terminal, backend::CrosstermBackend};
use tui::TerminalGuard;

fn main() -> Result<()> {
    let args = CliArgs::parse();
    let cwd = env::current_dir().context("resolve current directory")?;
    let mut host = HostdClient::spawn(args.hostd_command.clone(), args.hostd_args.clone())?;

    let mut terminal = TerminalGuard::enter()?;
    let initial_options = InitialOptions {
        model_id: args.model_id,
        provider: args.provider,
        api_key: args.api_key,
        thinking_level: args.thinking_level,
        session_name: args.session_name,
        no_tools: args.no_tools,
    };
    let mut app = AppState::new(cwd, args.session_id, args.continue_session, initial_options);
    let keymap = Keymap::load(&app.cwd());
    let exit_after = env::var("PIKO_TUI_EXIT_AFTER_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis);
    app.bootstrap(&mut host)?;

    let result = run_app(
        &mut terminal.terminal,
        &mut app,
        &mut host,
        &keymap,
        exit_after,
    );

    terminal.exit()?;
    host.shutdown();
    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AppState,
    host: &mut HostdClient,
    keymap: &Keymap,
    exit_after: Option<Duration>,
) -> Result<()> {
    let started = Instant::now();
    loop {
        for line in host.drain() {
            app.handle_host_line(host, line);
        }

        terminal
            .draw(|frame| render::render(frame, app))
            .context("draw terminal")?;

        if app.quit {
            return Ok(());
        }
        if let Some(exit_after) = exit_after
            && started.elapsed() >= exit_after
        {
            return Ok(());
        }

        if event::poll(Duration::from_millis(50)).context("poll terminal events")?
            && let CrosstermEvent::Key(key) = event::read().context("read terminal event")?
        {
            if let Some(action) = input::focus::InputRouter::route_key(app, keymap, key) {
                app.dispatch(host, action);
            }
        }

        if app.last_tick.elapsed() > Duration::from_millis(80) {
            app.last_tick = Instant::now();
            app.spinner_frame = app.spinner_frame.wrapping_add(1);
        }
    }
}
