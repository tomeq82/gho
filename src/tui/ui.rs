//! Top-level layout: top bar + two panels + bottom bar.
//!
//! `render` is called once per event-loop tick. It pulls the current
//! `AppState` and draws the whole frame in one go.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::{AppState, FocusPanel, LoadedImage, StatusLevel};
use crate::tui::theme::{Palette, Theme};
use crate::tui::widgets::hex;

/// Render a single frame.
pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let palette = state.theme.palette();

    // Three rows: top bar (3 lines), body (rest), bottom bar (1 line).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    render_top_bar(frame, chunks[0], state, &palette);
    render_body(frame, chunks[1], state, &palette);
    render_bottom_bar(frame, chunks[2], state, &palette);

    if state.show_help {
        render_help_overlay(frame, area, &palette);
    }
}

fn render_top_bar(frame: &mut Frame, area: Rect, state: &AppState, palette: &Palette) {
    let title = match state.mode {
        crate::tui::app::Mode::Browse => format!("gho browse: {}", display_inputs(&state.inputs)),
        crate::tui::app::Mode::Diff => format!(
            "gho diff: {} <-> {}",
            display_inputs(&state.inputs).split(' ').next().unwrap_or(""),
            display_inputs(&state.inputs).split(' ').nth(1).unwrap_or("")
        ),
    };
    let theme_label = match state.theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
    };
    let lines = vec![
        Line::from(Span::styled(title, palette.style_title())),
        Line::from(Span::styled(
            format!("theme: {theme_label} (press 't' to toggle)"),
            palette.style_dim(),
        )),
    ];
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::BOTTOM))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn render_body(frame: &mut Frame, area: Rect, state: &AppState, palette: &Palette) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_left_panel(frame, cols[0], state, palette);
    render_right_panel(frame, cols[1], state, palette);
}

fn render_left_panel(frame: &mut Frame, area: Rect, state: &AppState, palette: &Palette) {
    let focused = state.focus == FocusPanel::Left;
    let border_style = if focused {
        palette.style_border_focus()
    } else {
        palette.style_border_blur()
    };
    let (title, items) = match &state.image {
        Some(LoadedImage::Ghost11(img)) => {
            let title = format!(" Partitions ({}) ", img.partitions.len());
            let entries: Vec<ListItem> = img
                .partitions
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let fs = p.fs.to_string();
                    let size = human_bytes(p.summary.decompressed_bytes);
                    let mbr = p
                        .summary
                        .mbr_type
                        .map(|t| format!("0x{t:02X}"))
                        .unwrap_or_else(|| "—".to_string());
                    let line = format!(" {i}  {mbr}  {fs:<7} {size:>10}");
                    let style = if focused && i == img.selected {
                        palette.style_selection()
                    } else {
                        palette.style_base()
                    };
                    ListItem::new(Line::from(Span::styled(line, style)))
                })
                .collect();
            (title, entries)
        }
        Some(LoadedImage::GhostOld(_)) => {
            (" Pre-11.x (Week 3) ".to_string(), Vec::new())
        }
        None => {
            let title = " Image ".to_string();
            let entries: Vec<ListItem> = state
                .inputs
                .iter()
                .map(|p| {
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", p.display()),
                        palette.style_base(),
                    )))
                })
                .collect();
            (title, entries)
        }
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, palette.style_title()));
    frame.render_widget(block, area);
    let list = List::new(items)
        .style(palette.style_base())
        .highlight_style(palette.style_selection());
    frame.render_widget(list, inset(area));
}

fn render_right_panel(frame: &mut Frame, area: Rect, state: &AppState, palette: &Palette) {
    let focused = state.focus == FocusPanel::Right;
    let border_style = if focused {
        palette.style_border_focus()
    } else {
        palette.style_border_blur()
    };
    let title = match state.mode {
        crate::tui::app::Mode::Browse => " Detail ",
        crate::tui::app::Mode::Diff => " Right pane (diff) ",
    };

    // If a partition is selected and focus is on the right panel, show the
    // hex+ASCII preview of the partition's first 4 KB.
    if focused && matches!(state.mode, crate::tui::app::Mode::Browse) {
        if let Some(LoadedImage::Ghost11(img)) = &state.image {
            if let Some(p) = img.partitions.get(img.selected) {
                let path = &p.summary.output_path;
                let bytes = read_first_n(path, 4096).unwrap_or_default();
                let title = format!(" {} — {} ", p.fs, path.display());
                hex::render(
                    frame,
                    area,
                    &bytes,
                    state.hex_scroll,
                    palette,
                    title.trim(),
                );
                return;
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, palette.style_title()));
    frame.render_widget(block, area);

    let inner = inset(area);

    // Placeholder text when there's no image loaded yet.
    let content = if state.image.is_none() {
        vec![Line::from(Span::styled(
            if state.inputs.is_empty() {
                "(no input files)"
            } else {
                "loading... press '?' for keybindings"
            },
            palette.style_dim(),
        ))]
    } else if matches!(state.image, Some(LoadedImage::GhostOld(_))) {
        vec![Line::from(Span::styled(
            "dirent tree lands in Week 3",
            palette.style_dim(),
        ))]
    } else {
        vec![Line::from(Span::styled(
            "select a partition and press Tab →",
            palette.style_dim(),
        ))]
    };
    let p = Paragraph::new(content).style(palette.style_base()).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn read_first_n(path: &PathBuf, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let read = f.read(&mut buf)?;
    buf.truncate(read);
    Ok(buf)
}

fn human_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

fn render_bottom_bar(frame: &mut Frame, area: Rect, state: &AppState, palette: &Palette) {
    let (text, level) = match &state.status {
        Some(s) => (s.text.clone(), s.level),
        None => (String::new(), StatusLevel::Info),
    };
    let p = Paragraph::new(Line::from(Span::styled(
        text,
        Style::default().bg(palette.status_bg).fg(match level {
            StatusLevel::Info => palette.status_fg,
            StatusLevel::Ok => palette.ok,
            StatusLevel::Warn => palette.warn,
            StatusLevel::Error => palette.error,
        }),
    )));
    frame.render_widget(p, area);
}

fn render_help_overlay(frame: &mut Frame, area: Rect, palette: &Palette) {
    let popup = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.style_border_focus())
        .title(Span::styled(" Help ", palette.style_title()));
    frame.render_widget(block, popup);

    let text = vec![
        Line::from(Span::styled("gho TUI keys", palette.style_title())),
        Line::from(""),
        Line::from(Span::styled("  j / Down    move down", palette.style_base())),
        Line::from(Span::styled("  k / Up      move up", palette.style_base())),
        Line::from(Span::styled("  h / Left    collapse / back", palette.style_base())),
        Line::from(Span::styled("  l / Right   expand", palette.style_base())),
        Line::from(Span::styled("  Enter       open / expand", palette.style_base())),
        Line::from(Span::styled("  Tab         switch panel", palette.style_base())),
        Line::from(Span::styled("  F3          hex preview", palette.style_base())),
        Line::from(Span::styled("  F4          extract", palette.style_base())),
        Line::from(Span::styled("  Ctrl-F      search", palette.style_base())),
        Line::from(Span::styled("  t           toggle theme", palette.style_base())),
        Line::from(Span::styled("  ?           show / hide this help", palette.style_base())),
        Line::from(Span::styled("  q / Esc     quit", palette.style_base())),
        Line::from(Span::styled("  Ctrl-C      quit (forced)", palette.style_base())),
        Line::from(""),
        Line::from(Span::styled("press any key to dismiss", palette.style_dim())),
    ];
    let p = Paragraph::new(text).wrap(Wrap { trim: false });
    frame.render_widget(p, inset(popup));
}

fn display_inputs(inputs: &[PathBuf]) -> String {
    inputs
        .iter()
        .map(|p| {
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn inset(r: Rect) -> Rect {
    Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_shrinks_by_one_on_each_axis() {
        let r = Rect::new(0, 0, 20, 10);
        let i = inset(r);
        assert_eq!(i.x, 1);
        assert_eq!(i.y, 1);
        assert_eq!(i.width, 18);
        assert_eq!(i.height, 8);
    }

    #[test]
    fn centered_rect_is_inside_parent() {
        let r = Rect::new(0, 0, 100, 50);
        let p = centered_rect(60, 60, r);
        assert!(p.x + p.width <= r.width);
        assert!(p.y + p.height <= r.height);
        // Width and height should be near the requested percentage.
        assert!(p.width >= 50 && p.width <= 70);
    }

    #[test]
    fn display_inputs_shortens_to_filenames() {
        let inputs = vec![
            PathBuf::from("/tmp/foo/bar.gho"),
            PathBuf::from("/tmp/foo/baz.ghs"),
        ];
        assert_eq!(display_inputs(&inputs), "bar.gho, baz.ghs");
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 K");
        assert_eq!(human_bytes(1_572_864), "1.5 M");
        assert_eq!(human_bytes(1_073_741_824), "1.0 G");
    }
}
