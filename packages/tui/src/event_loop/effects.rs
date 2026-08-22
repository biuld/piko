use std::time::Instant;

use base64::Engine as _;

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
            Effect::ReadClipboardImage => match read_clipboard_image() {
                Ok((filename, data, mime_type)) => {
                    let follow_up = app.dispatch(
                        crate::app::command::EditorAction::InsertImage {
                            filename,
                            data,
                            mime_type,
                        }
                        .into(),
                    );
                    run_effects(app, host, follow_up);
                }
                Err(error) => app.push_error(format!("could not paste image: {error}")),
            },
            Effect::ReadImageFile {
                path,
                expected_draft,
            } => match read_image_file(&path) {
                Ok((filename, data, mime_type)) => {
                    let action = match expected_draft {
                        Some(expected_text) => {
                            crate::app::command::EditorAction::ReplaceDraftWithImage {
                                expected_text,
                                filename,
                                data,
                                mime_type,
                            }
                        }
                        None => crate::app::command::EditorAction::InsertImage {
                            filename,
                            data,
                            mime_type,
                        },
                    };
                    let follow_up = app.dispatch(action.into());
                    run_effects(app, host, follow_up);
                }
                Err(error) => app.push_error(format!("could not attach image: {error}")),
            },
        }
    }
}

fn read_image_file(path: &std::path::Path) -> Result<(String, String, String), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or("image path has no supported extension")?;
    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err(format!("unsupported image extension: {extension}")),
    };
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("image path has no valid filename")?
        .to_string();
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.is_empty() {
        return Err(format!("{} is empty", path.display()));
    }
    Ok((
        filename,
        base64::engine::general_purpose::STANDARD.encode(bytes),
        mime_type.into(),
    ))
}

fn read_clipboard_image() -> Result<(String, String, String), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    let image = clipboard.get_image().map_err(|error| error.to_string())?;
    let width = u32::try_from(image.width).map_err(|_| "clipboard image width is too large")?;
    let height = u32::try_from(image.height).map_err(|_| "clipboard image height is too large")?;
    let expected = image
        .width
        .checked_mul(image.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("clipboard image dimensions overflow")?;
    if image.bytes.len() != expected {
        return Err("clipboard image is not RGBA8".into());
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&image.bytes)
            .map_err(|error| error.to_string())?;
    }
    Ok((
        "clipboard.png".into(),
        base64::engine::general_purpose::STANDARD.encode(png_bytes),
        "image/png".into(),
    ))
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

#[cfg(test)]
mod image_file_tests {
    use super::*;

    #[test]
    fn reads_original_image_bytes_with_extension_mime_type() {
        let path =
            std::env::temp_dir().join(format!("piko-tui-image-{}.JPEG", uuid::Uuid::new_v4()));
        std::fs::write(&path, [0xff, 0xd8, 0xff]).unwrap();

        let result = read_image_file(&path);
        std::fs::remove_file(&path).unwrap();
        let (filename, data, mime_type) = result.unwrap();

        assert!(filename.ends_with(".JPEG"));
        assert_eq!(data, "/9j/");
        assert_eq!(mime_type, "image/jpeg");
    }
}
