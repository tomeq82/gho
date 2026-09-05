//! Central TUI application state.
//!
//! `AppState` owns everything the UI needs to render a frame:
//! - which mode we're in (browse vs diff)
//! - the loaded image metadata
//! - cursor position, scroll offset, focus panel
//! - any transient modal state (help overlay, search input, hex preview).
//!
//! The struct is `Clone` so that closures inside the event loop can
//! snapshot it without fighting the borrow checker.

use std::path::PathBuf;

use crate::tui::theme::Theme;

/// Which sub-app the TUI is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Single-image browser (`gho browse <image>`).
    Browse,
    /// Two-image snapshot diff (`gho diff <old> <new>`).
    Diff,
}

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Left,
    Right,
}

/// A status message shown in the bottom bar.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
    pub set_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Ok,
    Warn,
    Error,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self { text: text.into(), level: StatusLevel::Info, set_at: std::time::Instant::now() }
    }
    pub fn ok(text: impl Into<String>) -> Self {
        Self { text: text.into(), level: StatusLevel::Ok, set_at: std::time::Instant::now() }
    }
    pub fn warn(text: impl Into<String>) -> Self {
        Self { text: text.into(), level: StatusLevel::Warn, set_at: std::time::Instant::now() }
    }
    pub fn error(text: impl Into<String>) -> Self {
        Self { text: text.into(), level: StatusLevel::Error, set_at: std::time::Instant::now() }
    }
}

/// Top-level TUI state. All fields are public for direct access from
/// `ui.rs` and the event loop. Invariants are documented per-field.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Current sub-app.
    pub mode: Mode,

    /// Theme selection.
    pub theme: Theme,

    /// Whether the help overlay is currently shown.
    pub show_help: bool,

    /// Whether the application should exit on the next event-loop tick.
    pub should_quit: bool,

    /// Which panel has keyboard focus.
    pub focus: FocusPanel,

    /// Inputs the user provided on the command line.
    pub inputs: Vec<PathBuf>,

    /// Current status message (shown in the bottom bar).
    pub status: Option<StatusMessage>,
}

impl AppState {
    /// Build a fresh app state for `gho browse <inputs...>`.
    pub fn browse(inputs: Vec<PathBuf>) -> Self {
        Self {
            mode: Mode::Browse,
            theme: Theme::default(),
            show_help: false,
            should_quit: false,
            focus: FocusPanel::Left,
            inputs,
            status: Some(StatusMessage::info("loading...")),
        }
    }

    /// Build a fresh app state for `gho diff <old> <new>`.
    pub fn diff(inputs: Vec<PathBuf>) -> Self {
        Self {
            mode: Mode::Diff,
            theme: Theme::default(),
            show_help: false,
            should_quit: false,
            focus: FocusPanel::Left,
            inputs,
            status: Some(StatusMessage::info("loading...")),
        }
    }

    /// Toggle the theme.
    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.next();
        self.set_status(StatusMessage::info(match self.theme {
            Theme::Dark => "theme: dark",
            Theme::Light => "theme: light",
        }));
    }

    /// Replace the current status message.
    pub fn set_status(&mut self, msg: StatusMessage) {
        self.status = Some(msg);
    }

    /// Clear the status (used when a transient message times out).
    pub fn clear_status(&mut self) {
        self.status = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_state_starts_focused_left() {
        let s = AppState::browse(vec!["x.gho".into()]);
        assert_eq!(s.mode, Mode::Browse);
        assert_eq!(s.focus, FocusPanel::Left);
        assert!(!s.should_quit);
    }

    #[test]
    fn toggle_theme_flips_between_dark_and_light() {
        let mut s = AppState::browse(vec![]);
        assert_eq!(s.theme, Theme::Dark);
        s.toggle_theme();
        assert_eq!(s.theme, Theme::Light);
        s.toggle_theme();
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn set_status_replaces_previous() {
        let mut s = AppState::browse(vec![]);
        s.set_status(StatusMessage::info("first"));
        s.set_status(StatusMessage::error("second"));
        assert_eq!(s.status.as_ref().unwrap().text, "second");
        assert_eq!(s.status.as_ref().unwrap().level, StatusLevel::Error);
    }
}
