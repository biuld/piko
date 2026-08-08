//! Minimal shell split: reserve a chrome strip, hand remaining area to layout.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Which edge of the frame holds permanent chrome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ShellChrome {
    /// Bottom strip (typical status / bottom bar).
    Bottom { height: u16 },
    /// Top strip.
    Top { height: u16 },
}

/// Result of carving shell chrome out of the terminal frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellSplit {
    /// Area for the layout engine (flex plane + modals).
    pub body: Rect,
    /// Reserved chrome strip (client paints bottom bar etc.).
    pub chrome: Rect,
}

/// Split `frame` into body (for layout) and chrome rect.
pub fn split_shell(frame: Rect, chrome: ShellChrome) -> ShellSplit {
    match chrome {
        ShellChrome::Bottom { height } => {
            let h = height.min(frame.height);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(h)])
                .split(frame);
            ShellSplit {
                body: chunks[0],
                chrome: chunks[1],
            }
        }
        ShellChrome::Top { height } => {
            let h = height.min(frame.height);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(h), Constraint::Fill(1)])
                .split(frame);
            ShellSplit {
                chrome: chunks[0],
                body: chunks[1],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_chrome_leaves_body() {
        let split = split_shell(Rect::new(0, 0, 80, 24), ShellChrome::Bottom { height: 1 });
        assert_eq!(split.chrome, Rect::new(0, 23, 80, 1));
        assert_eq!(split.body, Rect::new(0, 0, 80, 23));
    }
}
