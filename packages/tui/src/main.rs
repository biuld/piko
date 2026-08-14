mod app;
mod cli;
mod config;
mod features;
mod host;
mod input;
mod layout;
mod navigation;
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
                    CrosstermEvent::Mouse(event) => {
                        let size = terminal.size().context("read terminal size")?;
                        let rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                        for action in input::pointer::route_pointer(app, rect, event) {
                            let effects = app.update(Msg::Action(action));
                            run_effects(app, host, effects);
                        }
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
            Effect::OpenUrl(url) => {
                if let Err(err) = open_url(&url) {
                    app.push_error(format!(
                        "could not open browser: {err}; open this URL manually: {url}"
                    ));
                }
            }
            Effect::CopyToClipboard {
                notification_id,
                text,
            } => match copy_to_clipboard(&text) {
                Ok(()) => {
                    app.notifications
                        .mark_copied(notification_id, Instant::now());
                    app.status = "notification copied".to_string();
                }
                Err(err) => app.push_error(format!("could not copy notification: {err}")),
            },
        }
    }
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    return write_to_clipboard_command("pbcopy", &[], text);

    #[cfg(target_os = "windows")]
    return write_to_clipboard_command("clip.exe", &[], text);

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let candidates: &[(&str, &[&str])] = &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ];
        let mut last_error = None;
        for (program, args) in candidates {
            match write_to_clipboard_command(program, args, text) {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(err),
            }
        }
        return Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no supported clipboard command found",
            )
        }));
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "clipboard is unsupported on this platform",
    ))
}

fn write_to_clipboard_command(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("clipboard command stdin unavailable"))?
        .write_all(text.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{program} exited with {status}"
        )))
    }
}

fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Process -FilePath $args[0]",
            url,
        ]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(|_| ())
}
