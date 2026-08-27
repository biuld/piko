//! Terminal runtime boundary for piko-tui.
//!
//! The rest of the TUI consumes the resolved profile and normalized events
//! from this module.  It never needs to inspect `TERM`, a multiplexer name, or
//! Crossterm's protocol-specific event details.

mod capability;
mod input;
mod policy;
mod profile;
mod session;
pub mod text;

pub use capability::{
    CapabilityDetector, ColorLevel, KeyboardEnhancements, Support, SystemCapabilityDetector,
    TerminalCapabilities,
};
pub use input::{InputNormalizer, KeyPhase, NormalizedInput, PointerEvent, PointerKind};
pub use policy::TerminalModePlan;
pub use profile::TerminalProfile;
pub use session::{TerminalSession, emergency_cleanup};

use anyhow::{Context, Result};

/// All ephemeral terminal state needed by the event loop.
pub struct TuiRuntime {
    pub session: TerminalSession,
    pub profile: TerminalProfile,
    pub input: InputNormalizer,
}

impl TuiRuntime {
    /// Detect capabilities and enter the terminal transactionally.
    pub fn enter() -> Result<Self> {
        let capabilities = SystemCapabilityDetector.detect();
        let requested_profile = TerminalProfile::resolve(capabilities);
        let session =
            TerminalSession::enter(requested_profile).context("enter terminal session")?;
        let profile = session.profile.clone();
        let input = InputNormalizer::new(profile.clone());
        Ok(Self {
            session,
            profile,
            input,
        })
    }

    pub fn exit(&mut self) -> Result<()> {
        self.session.exit()
    }
}
