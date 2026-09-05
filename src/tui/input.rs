//! Keyboard and mouse event → semantic action mapping.
//!
//! The TUI uses a single `Action` enum so the event loop can be a clean
//! `match` with no nested `if key.kind == KeyCode::Char('q') && modifiers == ...`
//! ladders.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// User actions. The event loop maps raw key/mouse events to these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Universal
    Quit,
    Help,
    ToggleTheme,
    Noop,

    // Movement
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Back,

    // Tabs / panel focus
    Tab,
    BackTab,

    // Browse mode
    Expand,
    Collapse,
    Preview,
    Extract,

    // Diff mode
    NextDiff,
    PrevDiff,
    SwitchSide,

    // Search
    Search,
    SearchNext,
    SearchPrev,

    // Mouse
    Click { column: u16, row: u16 },
    ScrollUp { column: u16, row: u16 },
    ScrollDown { column: u16, row: u16 },
}

pub fn map_key(key: KeyEvent) -> Action {
    // Ctrl-modified keys first (more specific).
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Action::Quit,
            KeyCode::Char('f') => Action::Search,
            KeyCode::Char('n') => Action::SearchNext,
            KeyCode::Char('p') => Action::SearchPrev,
            _ => Action::Noop,
        };
    }

    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,

        // Help
        KeyCode::Char('?') | KeyCode::F(1) => Action::Help,

        // Theme
        KeyCode::Char('t') => Action::ToggleTheme,

        // Movement
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Left | KeyCode::Char('h') => Action::Left,
        KeyCode::Right | KeyCode::Char('l') => Action::Right,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Home => Action::Home,
        KeyCode::End => Action::End,
        KeyCode::Enter => Action::Enter,
        KeyCode::Backspace => Action::Back,

        // Tabs
        KeyCode::Tab => Action::Tab,
        KeyCode::BackTab => Action::SwitchSide,

        // Browse
        KeyCode::F(3) => Action::Preview,
        KeyCode::F(4) => Action::Extract,

        // Diff
        KeyCode::Char('n') if !key.modifiers.is_empty() => Action::NextDiff,
        KeyCode::Char('p') if !key.modifiers.is_empty() => Action::PrevDiff,

        // Tree expand/collapse (also h/l in tree, but Enter and Right both
        // expand; Left collapses).
        _ => Action::Noop,
    }
}

pub fn map_mouse(mouse: MouseEvent) -> Action {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => Action::Click {
            column: mouse.column,
            row: mouse.row,
        },
        MouseEventKind::ScrollUp => Action::ScrollUp {
            column: mouse.column,
            row: mouse.row,
        },
        MouseEventKind::ScrollDown => Action::ScrollDown {
            column: mouse.column,
            row: mouse.row,
        },
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn quit_keys() {
        assert_eq!(map_key(k('q')), Action::Quit);
        assert_eq!(map_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)), Action::Quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(key), Action::Quit);
    }

    #[test]
    fn movement_keys() {
        assert_eq!(map_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)), Action::Up);
        assert_eq!(map_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)), Action::Down);
        assert_eq!(map_key(k('j')), Action::Down);
        assert_eq!(map_key(k('k')), Action::Up);
    }

    #[test]
    fn theme_toggle() {
        assert_eq!(map_key(k('t')), Action::ToggleTheme);
    }

    #[test]
    fn help_keys() {
        assert_eq!(map_key(k('?')), Action::Help);
        assert_eq!(map_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)), Action::Help);
    }

    #[test]
    fn search_keys() {
        let ctrl_f = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl_f), Action::Search);
    }
}
