//! Render smoke tests using ratatui's TestBackend.
//!
//! These don't drive the event loop (that needs a real tty) but they
//! exercise the layout code paths so we catch obvious regressions.

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::tui::app::{AppState, FocusPanel, Mode};
    use crate::tui::ui;

    fn render_to_string(state: &AppState) -> String {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| ui::render(frame, state))
            .unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn renders_browse_mode_with_image() {
        let state = AppState::browse(vec!["/tmp/foo.gho".into(), "/tmp/bar.ghs".into()]);
        let s = render_to_string(&state);
        // Top bar should contain the mode label "gho browse:".
        assert!(s.contains("gho browse:"), "missing title:\n{s}");
        // The two input filenames should appear in the left panel.
        assert!(s.contains("foo.gho"));
        assert!(s.contains("bar.ghs"));
        // No crash on empty right pane.
    }

    #[test]
    fn renders_diff_mode_with_two_images() {
        let state = AppState::diff(vec!["/tmp/old.gho".into(), "/tmp/new.gho".into()]);
        let s = render_to_string(&state);
        assert!(s.contains("gho diff:"), "missing title:\n{s}");
        assert_eq!(state.mode, Mode::Diff);
    }

    #[test]
    fn help_overlay_renders_over_main_view() {
        let mut state = AppState::browse(vec!["/tmp/x.gho".into()]);
        state.show_help = true;
        let s = render_to_string(&state);
        assert!(s.contains("gho TUI keys"), "help overlay missing:\n{s}");
        assert!(s.contains("F3"));
        assert!(s.contains("F4"));
    }

    #[test]
    fn theme_toggle_updates_palette() {
        let mut state = AppState::browse(vec!["/tmp/x.gho".into()]);
        let dark = render_to_string(&state);
        state.toggle_theme();
        let light = render_to_string(&state);
        // The two renderings should differ in colour escapes.
        assert_ne!(dark, light, "themes produced identical output");
    }

    #[test]
    fn focus_panel_changes_border_colour() {
        use ratatui::style::Color;
        let mut state = AppState::browse(vec!["/tmp/x.gho".into()]);
        state.focus = FocusPanel::Left;
        let backend_left = TestBackend::new(120, 30);
        let mut term_left = Terminal::new(backend_left).unwrap();
        term_left.draw(|frame| ui::render(frame, &state)).unwrap();
        // Top-left border cell of the right pane (the unfocused one).
        let buf_left = term_left.backend().buffer().clone();
        // Layout: 3 top rows + 2 panels (40% / 60%). Right panel starts at x=48.
        let right_border_top_left_x = 120 * 40 / 100; // 48
        let cell_right_border_left = buf_left
            .cell((right_border_top_left_x, 3))
            .map(|c| c.fg)
            .unwrap_or(Color::Reset);

        state.focus = FocusPanel::Right;
        let backend_right = TestBackend::new(120, 30);
        let mut term_right = Terminal::new(backend_right).unwrap();
        term_right.draw(|frame| ui::render(frame, &state)).unwrap();
        let buf_right = term_right.backend().buffer().clone();
        let cell_right_border_right = buf_right
            .cell((right_border_top_left_x, 3))
            .map(|c| c.fg)
            .unwrap_or(Color::Reset);

        assert_ne!(
            cell_right_border_left, cell_right_border_right,
            "right pane border colour should change when focus flips (left={:?}, right={:?})",
            cell_right_border_left, cell_right_border_right
        );
    }
}
