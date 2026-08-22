//! Client-local desktop presentation preferences.
//!
//! These values are deliberately outside hostd: they describe window
//! ergonomics, never session or runtime authority (ADR-022).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gpui::{Bounds, Pixels, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "desktop-prefs.json";
const MIN_WIDTH: f32 = 640.0;
const MIN_HEIGHT: f32 = 480.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WindowRect {
    fn sanitized(self) -> Option<Self> {
        let values = [self.x, self.y, self.width, self.height];
        if values.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(Self {
            width: self.width.max(MIN_WIDTH),
            height: self.height.max(MIN_HEIGHT),
            ..self
        })
    }

    pub fn into_bounds(self) -> Bounds<Pixels> {
        Bounds::new(
            point(px(self.x), px(self.y)),
            size(px(self.width), px(self.height)),
        )
    }

    pub fn from_window(bounds: WindowBounds) -> Self {
        let bounds = bounds.get_bounds();
        Self {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    pub window: Option<WindowRect>,
    /// User preference. Responsive collapse still wins below the breakpoint.
    pub sidebar_collapsed: bool,
    /// Presentation hint only; host discovery must confirm it before opening.
    pub last_session_id: Option<String>,
}

impl DesktopPrefs {
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let Ok(mut prefs) = serde_json::from_slice::<Self>(&bytes) else {
            return Self::default();
        };
        prefs.window = prefs.window.and_then(WindowRect::sanitized);
        prefs
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create desktop prefs directory {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self).context("encode desktop preferences")?;
        std::fs::write(path, bytes)
            .with_context(|| format!("write desktop preferences {}", path.display()))
    }
}

pub fn default_path() -> PathBuf {
    if let Some(root) = std::env::var_os("PIKO_HOME") {
        return PathBuf::from(root).join(FILE_NAME);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".piko")
        .join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_forward_compatible_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let prefs = DesktopPrefs {
            window: Some(WindowRect {
                x: 12.0,
                y: 24.0,
                width: 900.0,
                height: 700.0,
            }),
            sidebar_collapsed: true,
            last_session_id: Some("session-1".to_string()),
        };
        prefs.save(&path).unwrap();
        assert_eq!(DesktopPrefs::load(&path), prefs);

        std::fs::write(&path, r#"{"sidebar_collapsed":false,"future":1}"#).unwrap();
        assert_eq!(DesktopPrefs::load(&path), DesktopPrefs::default());
    }

    #[test]
    fn unsafe_window_sizes_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            r#"{"window":{"x":1.0,"y":2.0,"width":10.0,"height":20.0}}"#,
        )
        .unwrap();
        let window = DesktopPrefs::load(&path).window.unwrap();
        assert_eq!(window.width, MIN_WIDTH);
        assert_eq!(window.height, MIN_HEIGHT);
    }
}
