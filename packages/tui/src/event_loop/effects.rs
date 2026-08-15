use std::time::Instant;

use crate::{
    app::{AppState, effect::Effect},
    host::HostdClient,
};

pub(crate) fn run_effects(app: &mut AppState, host: &mut HostdClient, effects: Vec<Effect>) {
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
