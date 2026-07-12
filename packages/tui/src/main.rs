mod app;
mod cli;
mod config;
mod features;
mod host;
mod input;
mod layout;
mod render;
mod text;
mod theme;
mod tui;
mod ui;

use std::{
    env,
    io::Stdout,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use app::{
    AppState, InitialOptions,
    command::EditorAction,
    effect::{Effect, Msg},
};
use cli::CliArgs;
use crossterm::{
    SynchronizedUpdate,
    event::{self, Event as CrosstermEvent},
};
use host::HostdClient;
use input::keymap::Keymap;
use ratatui::{Terminal, backend::CrosstermBackend};
use tui::TerminalGuard;

fn main() -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let args = CliArgs::parse();
    let host_log = args.host_log_config();
    if !host_log.no_log
        && let Some(path) = &host_log.log_file
    {
        println!("Logging to {}", path.display());
    }
    let cwd = env::current_dir().context("resolve current directory")?;
    let mut host = HostdClient::spawn(
        args.hostd_command.clone(),
        args.hostd_args.clone(),
        &host_log,
    )?;

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
    let effects = app.bootstrap();
    run_effects(&mut app, &mut host, effects);

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
            let effects = app.update(Msg::HostLine(line));
            run_effects(app, host, effects);
        }

        std::io::stdout()
            .sync_update(|_| terminal.draw(|frame| render::render(frame, app)))
            .context("sync update terminal")?
            .context("draw terminal")?;

        if app.quit {
            return Ok(());
        }
        if let Some(exit_after) = exit_after
            && started.elapsed() >= exit_after
        {
            return Ok(());
        }

        if event::poll(Duration::from_millis(50)).context("poll terminal events")? {
            loop {
                match event::read().context("read terminal event")? {
                    CrosstermEvent::Key(key) => {
                        if let Some(action) = input::focus::InputRouter::route_key(app, keymap, key)
                        {
                            let effects = app.update(Msg::Action(action));
                            run_effects(app, host, effects);
                        }
                    }
                    CrosstermEvent::Paste(text) => {
                        let effects =
                            app.update(Msg::Action(EditorAction::InsertPaste(text).into()));
                        run_effects(app, host, effects);
                    }
                    _ => {}
                }

                if app.quit {
                    break;
                }

                // Batch events: if there are more events instantly available, process them before the next draw
                if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
                    break;
                }
            }
        }

        if app.last_tick.elapsed() > Duration::from_millis(80) {
            let effects = app.update(Msg::Tick);
            run_effects(app, host, effects);
        }
    }
}

fn run_effects(app: &mut AppState, host: &mut HostdClient, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Send(command) => {
                if let Err(err) = host.send(command) {
                    app.push_error(err.to_string());
                }
            }
        }
    }
}
