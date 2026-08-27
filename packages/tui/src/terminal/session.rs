use std::{
    io::{self, Write},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use super::{KeyboardEnhancements, TerminalModePlan, TerminalProfile};

/// One applied terminal mode. The transaction records it only after the
/// corresponding enable call succeeds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalMode {
    Raw,
    Keyboard(KeyboardEnhancements),
    AlternateScreen,
    BracketedPaste,
    MouseCapture,
    FocusEvents,
    SynchronizedOutput,
}

fn active_mode_journal() -> &'static Mutex<Vec<TerminalMode>> {
    static ACTIVE_MODES: OnceLock<Mutex<Vec<TerminalMode>>> = OnceLock::new();
    ACTIVE_MODES.get_or_init(|| Mutex::new(Vec::new()))
}

fn emergency_restored() -> &'static AtomicBool {
    static EMERGENCY_RESTORED: AtomicBool = AtomicBool::new(false);
    &EMERGENCY_RESTORED
}

fn clear_active_mode_journal() {
    if let Ok(mut active_modes) = active_mode_journal().lock() {
        active_modes.clear();
    }
}

/// Small I/O abstraction used by lifecycle fault-injection tests.
pub trait ModeIo {
    fn enable(&mut self, mode: TerminalMode) -> io::Result<()>;
    fn disable(&mut self, mode: TerminalMode) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
}

/// Applied-mode journal with rollback and idempotent cleanup.
#[derive(Debug, Default)]
pub struct ModeTransaction {
    applied: Vec<TerminalMode>,
    restored: bool,
}

impl ModeTransaction {
    #[allow(dead_code)]
    pub fn enter<I: ModeIo>(io: &mut I, plan: &TerminalModePlan) -> io::Result<Self> {
        let mut transaction = Self::default();
        for mode in planned_modes(plan) {
            if let Err(error) = io.enable(mode) {
                let _ = transaction.restore(io);
                return Err(error);
            }
            transaction.applied.push(mode);
        }
        Ok(transaction)
    }

    /// Enter the terminal while treating optional input enhancements as
    /// best-effort. Raw mode, the alternate screen, and any requested
    /// keyboard enhancement are still journaled; a rejected mouse or paste
    /// mode must not make the keyboard-only workflow unavailable.
    pub fn enter_with_optional<I: ModeIo>(io: &mut I, plan: &TerminalModePlan) -> io::Result<Self> {
        let mut transaction = Self::default();
        for mode in planned_modes(plan) {
            match io.enable(mode) {
                Ok(()) => transaction.applied.push(mode),
                Err(_error) if is_optional(mode) => {}
                Err(error) => {
                    let _ = transaction.restore(io);
                    return Err(error);
                }
            }
        }
        Ok(transaction)
    }

    pub fn restore<I: ModeIo>(&mut self, io: &mut I) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        if emergency_restored().load(Ordering::SeqCst) {
            self.applied.clear();
            self.restored = true;
            return Ok(());
        }
        let mut first_error = None;
        for mode in self.applied.drain(..).rev() {
            if let Err(error) = io.disable(mode)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Err(error) = io.show_cursor()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        self.restored = true;
        first_error.map_or(Ok(()), Err)
    }

    pub fn applied(&self) -> &[TerminalMode] {
        &self.applied
    }

    #[cfg(test)]
    pub fn is_restored(&self) -> bool {
        self.restored
    }
}

fn is_optional(mode: TerminalMode) -> bool {
    matches!(
        mode,
        TerminalMode::Keyboard(_)
            | TerminalMode::BracketedPaste
            | TerminalMode::MouseCapture
            | TerminalMode::FocusEvents
            | TerminalMode::SynchronizedOutput
    )
}

fn planned_modes(plan: &TerminalModePlan) -> Vec<TerminalMode> {
    let mut modes = vec![TerminalMode::Raw];
    if !plan.keyboard_flags.is_empty() {
        modes.push(TerminalMode::Keyboard(plan.keyboard_flags));
    }
    if plan.alternate_screen {
        modes.push(TerminalMode::AlternateScreen);
    }
    if plan.bracketed_paste {
        modes.push(TerminalMode::BracketedPaste);
    }
    if plan.mouse_capture {
        modes.push(TerminalMode::MouseCapture);
    }
    if plan.focus_events {
        modes.push(TerminalMode::FocusEvents);
    }
    if plan.synchronized_output {
        modes.push(TerminalMode::SynchronizedOutput);
    }
    modes
}

struct CrosstermModeIo<'a, W> {
    writer: &'a mut W,
}

impl<'a, W: Write> CrosstermModeIo<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self { writer }
    }
}

impl<W: Write> ModeIo for CrosstermModeIo<'_, W> {
    fn enable(&mut self, mode: TerminalMode) -> io::Result<()> {
        match mode {
            TerminalMode::Raw => enable_raw_mode(),
            TerminalMode::Keyboard(flags) => execute!(
                self.writer,
                PushKeyboardEnhancementFlags(to_crossterm_flags(flags))
            ),
            TerminalMode::AlternateScreen => execute!(self.writer, EnterAlternateScreen),
            TerminalMode::BracketedPaste => execute!(self.writer, EnableBracketedPaste),
            TerminalMode::MouseCapture => execute!(self.writer, EnableMouseCapture),
            TerminalMode::FocusEvents => execute!(self.writer, EnableFocusChange),
            // Crossterm does not expose synchronized output as a portable
            // command, so use the standard DEC private mode directly.
            TerminalMode::SynchronizedOutput => {
                self.writer.write_all(b"\x1b[?2026h")?;
                self.writer.flush()
            }
        }
    }

    fn disable(&mut self, mode: TerminalMode) -> io::Result<()> {
        match mode {
            TerminalMode::Raw => disable_raw_mode(),
            TerminalMode::Keyboard(_) => execute!(self.writer, PopKeyboardEnhancementFlags),
            TerminalMode::AlternateScreen => execute!(self.writer, LeaveAlternateScreen),
            TerminalMode::BracketedPaste => execute!(self.writer, DisableBracketedPaste),
            TerminalMode::MouseCapture => execute!(self.writer, DisableMouseCapture),
            TerminalMode::FocusEvents => execute!(self.writer, DisableFocusChange),
            TerminalMode::SynchronizedOutput => {
                self.writer.write_all(b"\x1b[?2026l")?;
                self.writer.flush()
            }
        }
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Show)
    }
}

/// Entered terminal plus its rollback journal.
pub struct TerminalSession {
    pub terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    transaction: ModeTransaction,
    pub profile: TerminalProfile,
}

impl TerminalSession {
    pub fn enter(profile: TerminalProfile) -> Result<Self> {
        // Constructing the backend is side-effect free.  Creating it before
        // changing terminal modes guarantees that every fallible operation
        // after mode activation is covered by the transaction journal.
        let stdout = std::io::stdout();
        let mut terminal =
            Terminal::new(CrosstermBackend::new(stdout)).context("create ratatui terminal")?;
        emergency_restored().store(false, Ordering::SeqCst);
        let transaction = {
            let mut io = CrosstermModeIo::new(terminal.backend_mut());
            ModeTransaction::enter_with_optional(&mut io, &profile.modes)
                .context("activate terminal modes")?
        };
        let active_keyboard_flags = transaction
            .applied()
            .iter()
            .find_map(|mode| match mode {
                TerminalMode::Keyboard(flags) => Some(*flags),
                _ => None,
            })
            .unwrap_or_else(KeyboardEnhancements::empty);
        let mut effective_profile = profile;
        effective_profile.active_keyboard_flags = active_keyboard_flags;
        effective_profile.key_reachability =
            if active_keyboard_flags.contains(KeyboardEnhancements::DISAMBIGUATE) {
                super::profile::KeyReachability::enhanced()
            } else {
                super::profile::KeyReachability::baseline()
            };
        if let Ok(mut active_modes) = active_mode_journal().lock() {
            *active_modes = transaction.applied.clone();
        }
        Ok(Self {
            terminal,
            transaction,
            profile: effective_profile,
        })
    }

    pub fn exit(&mut self) -> Result<()> {
        let result = {
            let mut io = CrosstermModeIo::new(self.terminal.backend_mut());
            self.transaction.restore(&mut io)
        };
        clear_active_mode_journal();
        result.context("restore terminal modes")?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let mut io = CrosstermModeIo::new(self.terminal.backend_mut());
        let _ = self.transaction.restore(&mut io);
        clear_active_mode_journal();
    }
}

fn to_crossterm_flags(flags: KeyboardEnhancements) -> KeyboardEnhancementFlags {
    let mut result = KeyboardEnhancementFlags::empty();
    if flags.contains(KeyboardEnhancements::DISAMBIGUATE) {
        result |= KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;
    }
    if flags.contains(KeyboardEnhancements::EVENT_TYPES) {
        result |= KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
    }
    if flags.contains(KeyboardEnhancements::ALTERNATE_KEYS) {
        result |= KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS;
    }
    result
}

/// Best-effort emergency restoration used by the panic hook. It uses the same
/// applied-mode journal as normal cleanup and is a no-op before activation.
pub fn emergency_cleanup() {
    let applied = active_mode_journal()
        .lock()
        .map(|mut active_modes| std::mem::take(&mut *active_modes))
        .unwrap_or_default();
    if applied.is_empty() {
        return;
    }
    emergency_restored().store(true, Ordering::SeqCst);
    let mut stdout = std::io::stdout();
    let mut io = CrosstermModeIo::new(&mut stdout);
    let mut transaction = ModeTransaction {
        applied,
        restored: false,
    };
    let _ = transaction.restore(&mut io);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeIo {
        events: Vec<String>,
        fail_enable_at: Option<usize>,
        enable_count: usize,
        fail_disable: bool,
    }

    impl ModeIo for FakeIo {
        fn enable(&mut self, mode: TerminalMode) -> io::Result<()> {
            let index = self.enable_count;
            self.enable_count += 1;
            if self.fail_enable_at == Some(index) {
                return Err(io::Error::other("injected enable failure"));
            }
            self.events.push(format!("enable:{mode:?}"));
            Ok(())
        }

        fn disable(&mut self, mode: TerminalMode) -> io::Result<()> {
            self.events.push(format!("disable:{mode:?}"));
            if self.fail_disable {
                Err(io::Error::other("injected disable failure"))
            } else {
                Ok(())
            }
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.events.push("show".to_string());
            Ok(())
        }
    }

    fn full_plan() -> TerminalModePlan {
        TerminalModePlan {
            keyboard_flags: KeyboardEnhancements::DISAMBIGUATE,
            mouse_capture: true,
            bracketed_paste: true,
            focus_events: true,
            synchronized_output: false,
            alternate_screen: true,
        }
    }

    #[test]
    fn activation_failure_rolls_back_successes_in_reverse_order() {
        let mut io = FakeIo {
            // Raw, keyboard, alternate succeed; bracketed paste fails.
            fail_enable_at: Some(3),
            ..Default::default()
        };
        assert!(ModeTransaction::enter(&mut io, &full_plan()).is_err());
        assert_eq!(
            io.events,
            vec![
                "enable:Raw",
                "enable:Keyboard(KeyboardEnhancements(1))",
                "enable:AlternateScreen",
                "disable:AlternateScreen",
                "disable:Keyboard(KeyboardEnhancements(1))",
                "disable:Raw",
                "show",
            ]
        );
    }

    #[test]
    fn optional_activation_failure_keeps_keyboard_workflow_available() {
        let mut io = FakeIo {
            // Raw, keyboard, and alternate screen succeed; bracketed paste
            // is optional and fails, while later optional modes still run.
            fail_enable_at: Some(3),
            ..Default::default()
        };
        let mut transaction = ModeTransaction::enter_with_optional(&mut io, &full_plan()).unwrap();

        assert!(transaction.applied().contains(&TerminalMode::Raw));
        assert!(
            transaction
                .applied()
                .contains(&TerminalMode::Keyboard(KeyboardEnhancements::DISAMBIGUATE))
        );
        assert!(
            transaction
                .applied()
                .contains(&TerminalMode::AlternateScreen)
        );
        assert!(
            !transaction
                .applied()
                .contains(&TerminalMode::BracketedPaste)
        );
        assert!(transaction.applied().contains(&TerminalMode::MouseCapture));
        assert!(transaction.applied().contains(&TerminalMode::FocusEvents));

        transaction.restore(&mut io).unwrap();
        assert!(
            !io.events
                .iter()
                .any(|event| event == "disable:BracketedPaste")
        );
    }

    #[test]
    fn restore_is_idempotent_and_attempts_every_disable() {
        let mut io = FakeIo::default();
        let mut transaction = ModeTransaction::enter(&mut io, &full_plan()).unwrap();
        transaction.restore(&mut io).unwrap();
        let after_first = io.events.clone();
        transaction.restore(&mut io).unwrap();
        assert_eq!(io.events, after_first);
        assert!(transaction.is_restored());
        assert!(transaction.applied().is_empty());
    }

    #[test]
    fn restore_returns_disable_error_after_best_effort_cleanup() {
        let mut io = FakeIo {
            fail_disable: true,
            ..Default::default()
        };
        let mut transaction = ModeTransaction::enter(&mut io, &full_plan()).unwrap();
        assert!(transaction.restore(&mut io).is_err());
        assert!(transaction.is_restored());
        assert!(io.events.iter().any(|event| event == "disable:Raw"));
        assert_eq!(io.events.last().map(String::as_str), Some("show"));
    }
}
