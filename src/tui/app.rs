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

    /// Loaded image — populated when the user opens a real image. We
    /// keep this as `Option<Box<dyn Any>>`-shaped via enum dispatch to
    /// keep the AppState `Clone`able for snapshotting.
    pub image: Option<LoadedImage>,

    /// Hex viewer's top-row byte offset. Zero means "from the start".
    pub hex_scroll: usize,

    /// First visible row in the diff list.
    pub diff_scroll: usize,
}

/// Either a 11.x image or a pre-11.x image. Each variant carries the
/// browse state plus enough metadata to render the partition / dirent
/// list without holding extra references into the parser.
#[derive(Debug, Clone)]
pub enum LoadedImage {
    Ghost11(crate::tui::browse::image11::Image11State),
    GhostOld(crate::tui::browse::image_old::ImageOldState),
    /// Two-image diff — see [`crate::tui::diff`].
    Diff(crate::tui::diff::Diff),
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
            image: None,
            hex_scroll: 0,
            diff_scroll: 0,
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
            image: None,
            hex_scroll: 0,
            diff_scroll: 0,
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

    /// Byte offset in the currently-previewed buffer (hex widget).
    pub fn hex_scroll(&self) -> usize {
        self.hex_scroll
    }

    /// Loaded image's first input path (None in early states).
    pub fn primary_input(&self) -> Option<&PathBuf> {
        self.inputs.first()
    }

    pub fn image11(&self) -> Option<&crate::tui::browse::image11::Image11State> {
        match &self.image {
            Some(LoadedImage::Ghost11(s)) => Some(s),
            _ => None,
        }
    }

    pub fn image11_mut(&mut self) -> Option<&mut crate::tui::browse::image11::Image11State> {
        match &mut self.image {
            Some(LoadedImage::Ghost11(s)) => Some(s),
            _ => None,
        }
    }

    pub fn image_old(&self) -> Option<&crate::tui::browse::image_old::ImageOldState> {
        match &self.image {
            Some(LoadedImage::GhostOld(s)) => Some(s),
            _ => None,
        }
    }

    pub fn image_old_mut(&mut self) -> Option<&mut crate::tui::browse::image_old::ImageOldState> {
        match &mut self.image {
            Some(LoadedImage::GhostOld(s)) => Some(s),
            _ => None,
        }
    }

    pub fn diff_view(&self) -> Option<&crate::tui::diff::Diff> {
        match &self.image {
            Some(LoadedImage::Diff(d)) => Some(d),
            _ => None,
        }
    }
}

/// Hex viewer's scroll byte offset. Kept on `AppState` so the event
/// loop can update it without juggling borrows.
pub const HEX_SCROLL_DEFAULT: usize = 0;

impl AppState {
    // Tiny helper so the field is on AppState without bloating the struct
    // with another option. We track the byte offset of the topmost row in
    // the hex viewer; `HEX_SCROLL_DEFAULT` (0) means "from the start".
    pub fn hex_scroll_set(&mut self, off: usize) {
        self.hex_scroll = off;
    }

    pub fn hex_scroll_bump(&mut self, delta: isize) {
        let new = (self.hex_scroll as isize + delta).max(0) as usize;
        self.hex_scroll = new;
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

    #[test]
    fn hex_scroll_default_and_bump() {
        let mut s = AppState::browse(vec![]);
        assert_eq!(s.hex_scroll(), 0);
        s.hex_scroll_bump(64);
        assert_eq!(s.hex_scroll(), 64);
        s.hex_scroll_bump(-32);
        assert_eq!(s.hex_scroll(), 32);
        s.hex_scroll_bump(-100);
        assert_eq!(s.hex_scroll(), 0, "should not underflow");
        s.hex_scroll_set(512);
        assert_eq!(s.hex_scroll(), 512);
    }
}
