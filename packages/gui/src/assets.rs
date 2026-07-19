//! Embedded SVG AssetSource for Workbench icons.

use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

/// Loads vendored icons from `packages/gui/assets/icons/`.
pub struct GuiAssets;

impl AssetSource for GuiAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "icons/plus.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/plus.svg"))),
            "icons/chevron-right.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron-right.svg"
            ))),
            "icons/chevron-down.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron-down.svg"
            ))),
            "icons/circle-dashed.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/circle-dashed.svg"
            ))),
            "icons/message-square.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/message-square.svg"
            ))),
            "icons/circle.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/circle.svg"))),
            "icons/bot.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/bot.svg"))),
            "icons/user.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/user.svg"))),
            "icons/folder.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/folder.svg"))),
            "icons/folder-open.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/folder-open.svg"
            ))),
            "icons/wrench.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/wrench.svg"))),
            "icons/brain.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/brain.svg"))),
            "icons/cpu.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/cpu.svg"))),
            "icons/git-branch.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/git-branch.svg"
            ))),
            "icons/layers.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/layers.svg"))),
            "icons/network.svg" => {
                Some(Cow::Borrowed(include_bytes!("../assets/icons/network.svg")))
            }
            "icons/triangle-alert.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/triangle-alert.svg"
            ))),
            "icons/send.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/send.svg"))),
            "icons/circle-stop.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/circle-stop.svg"
            ))),
            "icons/activity.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/activity.svg"
            ))),
            "icons/bell.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/bell.svg"))),
            "icons/inbox.svg" => Some(Cow::Borrowed(include_bytes!("../assets/icons/inbox.svg"))),
            "icons/settings.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/settings.svg"
            ))),
            "icons/panel-left.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-left.svg"
            ))),
            "icons/panel-left-filled.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-left-filled.svg"
            ))),
            "icons/panel-right.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-right.svg"
            ))),
            "icons/panel-right-filled.svg" => Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/panel-right-filled.svg"
            ))),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let trimmed = path.trim_end_matches('/');
        if trimmed == "icons" || trimmed.is_empty() {
            Ok(vec![
                "plus.svg".into(),
                "chevron-right.svg".into(),
                "chevron-down.svg".into(),
                "circle-dashed.svg".into(),
                "message-square.svg".into(),
                "circle.svg".into(),
                "bot.svg".into(),
                "user.svg".into(),
                "folder.svg".into(),
                "folder-open.svg".into(),
                "wrench.svg".into(),
                "brain.svg".into(),
                "cpu.svg".into(),
                "git-branch.svg".into(),
                "layers.svg".into(),
                "network.svg".into(),
                "triangle-alert.svg".into(),
                "send.svg".into(),
                "circle-stop.svg".into(),
                "activity.svg".into(),
                "bell.svg".into(),
                "inbox.svg".into(),
                "settings.svg".into(),
                "panel-left.svg".into(),
                "panel-left-filled.svg".into(),
                "panel-right.svg".into(),
                "panel-right-filled.svg".into(),
            ])
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_vendored_plus() {
        let data = GuiAssets.load("icons/plus.svg").unwrap().unwrap();
        assert!(std::str::from_utf8(&data).unwrap().contains("lucide"));
    }

    #[test]
    fn loads_panel_filled_icons() {
        for path in [
            "icons/panel-left-filled.svg",
            "icons/panel-right-filled.svg",
        ] {
            let data = GuiAssets.load(path).unwrap().unwrap();
            assert!(
                std::str::from_utf8(&data).unwrap().contains("clipPath"),
                "{path} should embed hatch clip"
            );
        }
    }

    #[test]
    fn lists_icon_dir() {
        let names = GuiAssets.list("icons").unwrap();
        assert!(names.iter().any(|n| n.as_ref() == "plus.svg"));
        assert!(names.iter().any(|n| n.as_ref() == "panel-left-filled.svg"));
        assert!(names.iter().any(|n| n.as_ref() == "panel-right-filled.svg"));
    }
}
