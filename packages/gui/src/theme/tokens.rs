//! Piko semantic theme tokens (dark-first).

use gpui::{Hsla, Rgba, rgb};

/// Fleet Dark-derived semantic palette for the editor-style Workbench.
#[derive(Debug, Clone, Copy)]
pub struct PikoTokens {
    pub canvas: u32,
    pub surface: u32,
    pub elevated: u32,
    pub chrome: u32,
    pub fg: u32,
    pub muted_fg: u32,
    pub border: u32,
    pub ring: u32,
    pub accent: u32,
    pub success: u32,
    pub warning: u32,
    pub danger: u32,
    pub info: u32,
    pub user: u32,
    pub assistant: u32,
    pub thinking: u32,
    pub tool: u32,
    pub system: u32,
}

impl PikoTokens {
    pub const fn dark() -> Self {
        Self {
            canvas: 0x090909,
            surface: 0x18191b,
            elevated: 0x252629,
            chrome: 0x090909,
            fg: 0xe0e1e4,
            muted_fg: 0x898e94,
            border: 0x3e4147,
            ring: 0x2a7deb,
            accent: 0x4b8dec,
            success: 0x169068,
            warning: 0xb07203,
            danger: 0xe1465e,
            info: 0x4b8dec,
            user: 0x87c3ff,
            assistant: 0x82d2ce,
            thinking: 0x909194,
            tool: 0xebc88d,
            system: 0x6e747b,
        }
    }

    pub fn rgba(hex: u32) -> Rgba {
        rgb(hex)
    }

    pub fn hsla(hex: u32) -> Hsla {
        Hsla::from(rgb(hex))
    }

    pub fn canvas_rgba(self) -> Rgba {
        Self::rgba(self.canvas)
    }

    pub fn surface_rgba(self) -> Rgba {
        Self::rgba(self.surface)
    }

    pub fn elevated_rgba(self) -> Rgba {
        Self::rgba(self.elevated)
    }

    pub fn chrome_rgba(self) -> Rgba {
        Self::rgba(self.chrome)
    }

    pub fn fg_rgba(self) -> Rgba {
        Self::rgba(self.fg)
    }

    pub fn muted_fg_rgba(self) -> Rgba {
        Self::rgba(self.muted_fg)
    }

    pub fn border_rgba(self) -> Rgba {
        Self::rgba(self.border)
    }

    pub fn ring_rgba(self) -> Rgba {
        Self::rgba(self.ring)
    }

    pub fn role_accent(self, role: RoleAccent) -> Rgba {
        Self::rgba(self.role_hex(role))
    }

    pub fn role_accent_hsla(self, role: RoleAccent) -> Hsla {
        Self::hsla(self.role_hex(role))
    }

    fn role_hex(self, role: RoleAccent) -> u32 {
        match role {
            RoleAccent::User => self.user,
            RoleAccent::Assistant => self.assistant,
            RoleAccent::Thinking => self.thinking,
            RoleAccent::Tool => self.tool,
            RoleAccent::System => self.system,
            RoleAccent::Success => self.success,
            RoleAccent::Warning => self.warning,
            RoleAccent::Danger => self.danger,
            RoleAccent::Info => self.info,
            RoleAccent::Accent => self.accent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleAccent {
    User,
    Assistant,
    /// Thinking chrome (timeline quotes + Tree thinking-level nodes).
    Thinking,
    Tool,
    System,
    Success,
    Warning,
    Danger,
    Info,
    Accent,
}

/// Process-wide dark tokens for render helpers that lack `App` access.
pub fn tokens() -> PikoTokens {
    PikoTokens::dark()
}
