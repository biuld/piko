mod app;
#[cfg(test)]
mod architecture_tests;
mod cli;
mod config;
mod event_loop;
mod features;
mod host;
mod input;
mod layout;
mod navigation;
mod render;
mod terminal;
mod text;
mod theme;
mod ui;

use std::{env, time::Duration};

use anyhow::{Context, Result};
use app::{AppState, InitialOptions};
use cli::CliArgs;
use host::HostdClient;
use terminal::{TuiRuntime, emergency_cleanup};

fn main() -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        emergency_cleanup();
        original_hook(panic_info);
    }));

    let args = CliArgs::parse();
    let host_log = args.host_log_config();
    let cwd = env::current_dir().context("resolve current directory")?;
    let mut host = HostdClient::spawn(
        args.hostd_command.clone(),
        args.hostd_args.clone(),
        host_log.log_level.as_deref(),
    )?;

    let mut runtime = TuiRuntime::enter()?;
    let initial_options = InitialOptions {
        model_id: args.model_id,
        provider: args.provider,
        api_key: args.api_key,
        thinking_level: args.thinking_level,
        session_name: args.session_name,
        no_tools: args.no_tools,
    };
    let mut app = AppState::new(cwd, args.session_id, args.continue_session, initial_options);
    app.configure_terminal_profile(runtime.profile.clone());
    let exit_after = env::var("PIKO_TUI_EXIT_AFTER_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis);
    let effects = app.bootstrap();
    event_loop::run_bootstrap_effects(&mut app, &mut host, effects);

    let result = event_loop::run(
        &mut runtime.session.terminal,
        &mut app,
        &mut host,
        &runtime.input,
        exit_after,
    );

    runtime.exit()?;
    host.shutdown();
    result
}
