//! Render smoke tests using ratatui's TestBackend.
//!
//! These don't drive the event loop (that needs a real tty) but they
//! exercise the layout code paths so we catch obvious regressions.

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::ghost11::stream::{ExtractResult, PartitionSummary};
    // FileHeader is re-exported from gho::ghost11
    use crate::ghost11::FileHeader;
    use crate::tui::app::{AppState, FocusPanel, LoadedImage, Mode};
    use crate::tui::browse::image11::{Image11State, PartitionEntry};
    use crate::tui::browse::fs_detect::FsKind;
    use crate::tui::ui;
    use std::path::PathBuf;

    fn render_to_string(state: &AppState) -> String {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| ui::render(frame, state))
            .unwrap();
        terminal.backend().to_string()
    }

    /// Build a synthetic 11.x ExtractResult for rendering tests.
    fn fake_extract() -> ExtractResult {
        ExtractResult {
            header: FileHeader {
                file_type: 1,
                compression: 0,
                image_id: 0xDEADBEEF,
                encrypted: false,
            },
            mbr_entries: vec![],
            partitions: vec![
                PartitionSummary {
                    index: 0,
                    mbr_type: Some(0x07), // NTFS
                    compressed_bytes: 1024,
                    decompressed_bytes: 4096,
                    output_path: PathBuf::from("/tmp/p0.img"),
                },
                PartitionSummary {
                    index: 1,
                    mbr_type: Some(0x82), // Linux swap
                    compressed_bytes: 50,
                    decompressed_bytes: 256,
                    output_path: PathBuf::from("/tmp/p1.img"),
                },
            ],
        }
    }

    #[test]
    fn renders_browse_mode_with_image() {
        let state = AppState::browse(vec!["/tmp/foo.gho".into(), "/tmp/bar.ghs".into()]);
        let s = render_to_string(&state);
        assert!(s.contains("gho browse:"), "missing title:\n{s}");
        assert!(s.contains("foo.gho"));
        assert!(s.contains("bar.ghs"));
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
        let buf_left = term_left.backend().buffer().clone();
        let right_border_top_left_x = 120 * 40 / 100;
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

    #[test]
    fn partition_list_renders_for_11x_image() {
        // Build an Image11State from a fake extract.
        let tmp = tempfile::tempdir().unwrap();
        let p0_path = tmp.path().join("p0.img");
        let p1_path = tmp.path().join("p1.img");
        // NTFS-like boot sector (full 512 bytes — detect_fs rejects shorter).
        let mut ntfs_bs = vec![0u8; 512];
        ntfs_bs[0..8].copy_from_slice(b"NTFS    ");
        ntfs_bs[510] = 0x55;
        ntfs_bs[511] = 0xAA;
        std::fs::write(&p0_path, &ntfs_bs).unwrap();
        // MBR-only signature
        let mut p1 = vec![0u8; 512];
        p1[510] = 0x55;
        p1[511] = 0xAA;
        std::fs::write(&p1_path, &p1).unwrap();

        let mut extract = fake_extract();
        extract.partitions[0].output_path = p0_path.clone();
        extract.partitions[1].output_path = p1_path.clone();

        let image11 = Image11State::load(PathBuf::from("/tmp/test.gho"), extract).unwrap();
        // Sanity: detect_fs correctly identifies both partitions.
        assert_eq!(image11.partitions[0].fs, FsKind::Ntfs);
        assert_eq!(
            image11.partitions[1].fs,
            FsKind::BootSector(crate::tui::browse::fs_detect::BootLoader::MbrBlank)
        );

        let mut state = AppState::browse(vec!["/tmp/test.gho".into()]);
        state.image = Some(LoadedImage::Ghost11(image11));
        let s = render_to_string(&state);
        // Partition list should mention both indices and the detected FS.
        assert!(s.contains("NTFS"), "expected NTFS label in rendered output:\n{s}");
        assert!(s.contains("MBR"), "expected MBR label in rendered output:\n{s}");
    }

    #[test]
    fn movement_actions_move_cursor() {
        let mut state = AppState::browse(vec!["/tmp/x.gho".into()]);
        // Inject two partitions directly.
        let image11 = Image11State {
            source_path: PathBuf::from("/tmp/x.gho"),
            partitions: vec![
                PartitionEntry {
                    summary: PartitionSummary {
                        index: 0,
                        mbr_type: Some(0x07),
                        compressed_bytes: 100,
                        decompressed_bytes: 400,
                        output_path: PathBuf::from("/tmp/p0"),
                    },
                    fs: FsKind::Ntfs,
                },
                PartitionEntry {
                    summary: PartitionSummary {
                        index: 1,
                        mbr_type: Some(0x82),
                        compressed_bytes: 50,
                        decompressed_bytes: 200,
                        output_path: PathBuf::from("/tmp/p1"),
                    },
                    fs: FsKind::Swap,
                },
            ],
            selected: 0,
            scroll: 0,
        };
        state.image = Some(LoadedImage::Ghost11(image11));
        // Simulate Down → Enter → Down key sequences by calling handle_action.
        use crate::tui::Action;
        use crate::tui::handle_action;
        handle_action(&mut state, Action::Down);
        assert_eq!(state.image11().unwrap().selected, 1);
        handle_action(&mut state, Action::Down);
        assert_eq!(state.image11().unwrap().selected, 1, "should clamp at last");
        handle_action(&mut state, Action::Up);
        assert_eq!(state.image11().unwrap().selected, 0);
        handle_action(&mut state, Action::Home);
        assert_eq!(state.image11().unwrap().selected, 0);
        handle_action(&mut state, Action::End);
        assert_eq!(state.image11().unwrap().selected, 1);
        handle_action(&mut state, Action::Up);
        assert_eq!(state.image11().unwrap().selected, 0);
    }
}
