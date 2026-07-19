//! Vendored Lucide-compatible icons for Workbench chrome.

use gpui::{Hsla, IntoElement, ParentElement, Pixels, SharedString, Styled, div, px};
use gpui_component::{Icon, IconNamed, Sizable, Size};

/// Fixed icon box sizes aligned to typography roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSize {
    /// 12 px — disclosure / inline chrome (Meta).
    Meta,
    /// 14 px — header ghost actions (Label).
    Label,
    /// 28 px — Empty / Loading placeholder mark.
    Placeholder,
}

impl IconSize {
    pub fn pixels(self) -> Pixels {
        match self {
            Self::Meta => px(12.),
            Self::Label => px(14.),
            Self::Placeholder => px(28.),
        }
    }
}

/// Typed subset of Lucide icons used by piko-gui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PikoIcon {
    Plus,
    ChevronRight,
    ChevronDown,
    CircleDashed,
    MessageSquare,
    Circle,
    Bot,
    User,
    Folder,
    FolderOpen,
    Wrench,
    Brain,
    Cpu,
    GitBranch,
    Layers,
    Network,
    TriangleAlert,
    Send,
    CircleStop,
    Activity,
    Bell,
    Inbox,
    /// Gear / settings — used as the streaming Assistant spinner.
    Settings,
    /// TitleBar Sessions toggle — hollow when the column is closed.
    PanelLeft,
    /// TitleBar Sessions toggle — filled pane when the column is docked.
    PanelLeftFilled,
    /// TitleBar Agents/Tree toggle — hollow when the column is closed.
    PanelRight,
    /// TitleBar Agents/Tree toggle — filled pane when the column is docked.
    PanelRightFilled,
}

impl IconNamed for PikoIcon {
    fn path(self) -> SharedString {
        match self {
            Self::Plus => "icons/plus.svg",
            Self::ChevronRight => "icons/chevron-right.svg",
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::CircleDashed => "icons/circle-dashed.svg",
            Self::MessageSquare => "icons/message-square.svg",
            Self::Circle => "icons/circle.svg",
            Self::Bot => "icons/bot.svg",
            Self::User => "icons/user.svg",
            Self::Folder => "icons/folder.svg",
            Self::FolderOpen => "icons/folder-open.svg",
            Self::Wrench => "icons/wrench.svg",
            Self::Brain => "icons/brain.svg",
            Self::Cpu => "icons/cpu.svg",
            Self::GitBranch => "icons/git-branch.svg",
            Self::Layers => "icons/layers.svg",
            Self::Network => "icons/network.svg",
            Self::TriangleAlert => "icons/triangle-alert.svg",
            Self::Send => "icons/send.svg",
            Self::CircleStop => "icons/circle-stop.svg",
            Self::Activity => "icons/activity.svg",
            Self::Bell => "icons/bell.svg",
            Self::Inbox => "icons/inbox.svg",
            Self::Settings => "icons/settings.svg",
            Self::PanelLeft => "icons/panel-left.svg",
            Self::PanelLeftFilled => "icons/panel-left-filled.svg",
            Self::PanelRight => "icons/panel-right.svg",
            Self::PanelRightFilled => "icons/panel-right-filled.svg",
        }
        .into()
    }
}

/// Build a tinted, sized icon element.
pub fn icon(name: PikoIcon, size: IconSize, color: impl Into<Hsla>) -> Icon {
    Icon::new(name)
        .with_size(Size::Size(size.pixels()))
        .text_color(color.into())
}

/// Fixed-width leading mark for tree / list rows (aligns with Meta icon size).
pub fn row_leading(name: PikoIcon, color: impl Into<Hsla>) -> gpui::AnyElement {
    div()
        .w(px(16.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(icon(name, IconSize::Meta, color))
        .into_any_element()
}

/// Rotating gear mark for streaming Assistant (gear is meant to spin).
///
/// Reduced motion shows a static gear instead of animating.
pub fn rotating_gear(color: impl Into<Hsla>, animate: bool) -> gpui::AnyElement {
    let color = color.into();
    let mark: gpui::AnyElement = if animate {
        gpui_component::spinner::Spinner::new()
            .icon(Icon::new(PikoIcon::Settings))
            .with_size(Size::Size(IconSize::Meta.pixels()))
            .color(color)
            .into_any_element()
    } else {
        icon(PikoIcon::Settings, IconSize::Meta, color).into_any_element()
    };
    div()
        .w(px(16.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
        .into_any_element()
}

/// Disclosure chevron for expanded / collapsed rows.
pub fn disclosure(expanded: bool, color: impl Into<Hsla>) -> Icon {
    icon(
        if expanded {
            PikoIcon::ChevronDown
        } else {
            PikoIcon::ChevronRight
        },
        IconSize::Meta,
        color,
    )
}

/// TitleBar panel toggle: hollow when closed, hatched pane when docked (Fleet).
pub fn panel_toggle_icon(side: PanelSide, docked: bool, color: impl Into<Hsla>) -> Icon {
    let name = match (side, docked) {
        (PanelSide::Left, false) => PikoIcon::PanelLeft,
        (PanelSide::Left, true) => PikoIcon::PanelLeftFilled,
        (PanelSide::Right, false) => PikoIcon::PanelRight,
        (PanelSide::Right, true) => PikoIcon::PanelRightFilled,
    };
    icon(name, IconSize::Label, color)
}

/// Which outer Workbench column a panel toggle controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

/// Placeholder mark for Empty / Loading islands.
pub fn placeholder_icon(name: PikoIcon, color: impl Into<Hsla>) -> impl IntoElement {
    icon(name, IconSize::Placeholder, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_stable() {
        assert_eq!(PikoIcon::Plus.path().as_ref(), "icons/plus.svg");
        assert_eq!(PikoIcon::Bot.path().as_ref(), "icons/bot.svg");
        assert_eq!(PikoIcon::User.path().as_ref(), "icons/user.svg");
        assert_eq!(PikoIcon::Settings.path().as_ref(), "icons/settings.svg");
        assert_eq!(PikoIcon::Folder.path().as_ref(), "icons/folder.svg");
        assert_eq!(PikoIcon::Network.path().as_ref(), "icons/network.svg");
        assert_eq!(PikoIcon::PanelLeft.path().as_ref(), "icons/panel-left.svg");
        assert_eq!(
            PikoIcon::PanelLeftFilled.path().as_ref(),
            "icons/panel-left-filled.svg"
        );
        assert_eq!(
            PikoIcon::PanelRightFilled.path().as_ref(),
            "icons/panel-right-filled.svg"
        );
        assert_eq!(IconSize::Placeholder.pixels(), px(28.));
    }
}
