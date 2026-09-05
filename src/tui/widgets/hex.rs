//! Hex+ASCII viewer widget.
//!
//! Renders a fixed-size window over a byte slice. 16 bytes per row:
//! - left column: 8-hex-digit offset (e.g. `00000000`)
//! - middle: 16 bytes as 2-character hex groups separated by a gap
//! - right: printable ASCII or `.` for non-printable
//!
//! The widget does not own the buffer — callers pass `&[u8]` at render
//! time so the underlying data can be loaded lazily (read from disk,
//! streamed from a compressed block, etc.) without re-uploading.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::theme::Palette;

/// Render a hex+ASCII view of `data` (or a placeholder if `data` is empty).
///
/// `scroll` is the byte offset of the first row to show. `viewport_rows`
/// should match the height of the inner area of the surrounding block.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    data: &[u8],
    scroll: usize,
    palette: &Palette,
    title: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.style_border_blur())
        .title(Span::styled(format!(" {title} "), palette.style_title()));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if data.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            "(no data — press F3 to preview a partition or file)",
            palette.style_dim(),
        )))
        .wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
        return;
    }

    let lines = build_hex_lines(data, scroll, inner.height as usize, inner.width, palette);
    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn build_hex_lines<'a>(
    data: &'a [u8],
    scroll: usize,
    max_rows: usize,
    width: u16,
    palette: &Palette,
) -> Vec<Line<'a>> {
    let bytes_per_row = 16usize;
    let total_rows = data.len().div_ceil(bytes_per_row);
    let mut start_row = scroll / bytes_per_row;
    if start_row >= total_rows {
        start_row = total_rows.saturating_sub(1);
    }
    let end_row = (start_row + max_rows).min(total_rows);

    // Available width determines how many columns of the hex+ASCII strip
    // we can render. 16 bytes fit in: 10 (offset) + 1 (space) + 16*3 (hex
    // groups) + 2 (gaps) + 2 (space) + 16 (ASCII) = 79 cols. Wider areas
    // just get more whitespace.
    let _ = width;

    (start_row..end_row)
        .map(|row| {
            let offset = row * bytes_per_row;
            let end = (offset + bytes_per_row).min(data.len());
            let chunk = &data[offset..end];

            let offset_str = format!("{offset:08x}");
            let mut hex = String::with_capacity(bytes_per_row * 3);
            for (i, b) in chunk.iter().enumerate() {
                if i == 8 {
                    hex.push(' ');
                }
                hex.push_str(&format!(" {b:02x}"));
            }
            // Pad short last row so ASCII column lines up.
            for _ in 0..(bytes_per_row - chunk.len()) {
                hex.push_str("   ");
            }
            if chunk.len() <= 8 {
                hex.push(' ');
            }

            let mut ascii = String::with_capacity(bytes_per_row);
            for b in chunk {
                if (0x20..0x7F).contains(b) {
                    ascii.push(*b as char);
                } else {
                    ascii.push('.');
                }
            }

            Line::from(vec![
                Span::styled(offset_str, palette.style_dim()),
                Span::raw("  "),
                Span::styled(hex, palette.style_base()),
                Span::raw("  "),
                Span::styled(ascii, palette.style_base()),
            ])
        })
        .collect()
}

/// Total number of rows the data occupies. Used by the scroll logic.
pub fn row_count(data_len: usize) -> usize {
    data_len.div_ceil(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_returns_zero_rows() {
        assert_eq!(row_count(0), 0);
    }

    #[test]
    fn row_count_rounds_up() {
        assert_eq!(row_count(1), 1);
        assert_eq!(row_count(16), 1);
        assert_eq!(row_count(17), 2);
        assert_eq!(row_count(32), 2);
    }

    #[test]
    fn build_lines_respects_scroll() {
        let data: Vec<u8> = (0..=255).cycle().take(64).collect();
        let p = ratatui::style::Color::Reset;
        let palette = Palette {
            bg: p, fg: p, border_focus: p, border_blur: p, title: p,
            selection_bg: p, selection_fg: p, status_bg: p, status_fg: p,
            modified: p, added: p, removed: p, dim: p, dir: p, file: p,
            archive: p, vfat_long: p, error: p, warn: p, ok: p,
        };
        let lines = build_hex_lines(&data, 16, 4, 80, &palette);
        // 64 bytes → 4 rows. Scroll by 16 = 1 row → 3 rows remain.
        assert_eq!(lines.len(), 3);
        let first: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.contains("00000010"), "second row should start at offset 0x10, got: {first}");
    }

    #[test]
    fn printable_bytes_render_as_ascii() {
        let data: Vec<u8> = (b'A'..=b'Z').collect();
        let p = ratatui::style::Color::Reset;
        let palette = Palette {
            bg: p, fg: p, border_focus: p, border_blur: p, title: p,
            selection_bg: p, selection_fg: p, status_bg: p, status_fg: p,
            modified: p, added: p, removed: p, dim: p, dir: p, file: p,
            archive: p, vfat_long: p, error: p, warn: p, ok: p,
        };
        let lines = build_hex_lines(&data, 0, 1, 80, &palette);
        let ascii: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        // The last span is the ASCII column. The middle hex column may
        // also contain digits; just look for the trailing letters.
        assert!(ascii.contains("ABCDEFGHIJKLMNOP"), "expected ASCII letters, got: {ascii}");
    }

    #[test]
    fn non_printable_bytes_render_as_dot() {
        let data = vec![0x00u8, 0xFFu8, 0x7Fu8, 0x80u8];
        let p = ratatui::style::Color::Reset;
        let palette = Palette {
            bg: p, fg: p, border_focus: p, border_blur: p, title: p,
            selection_bg: p, selection_fg: p, status_bg: p, status_fg: p,
            modified: p, added: p, removed: p, dim: p, dir: p, file: p,
            archive: p, vfat_long: p, error: p, warn: p, ok: p,
        };
        let lines = build_hex_lines(&data, 0, 1, 80, &palette);
        let all: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(all.contains("...."), "non-printable bytes should render as '.', got: {all}");
    }
}
