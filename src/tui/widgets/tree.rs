//! Tree view widget — renders a hierarchical dirent tree with
//! collapse/expand markers.
//!
//! Render-only: takes a list of visible node indices (produced by
//! `DirentTree::visible_indices`) and the `expanded` set, draws each
//! row with an appropriate indent and a triangle marker for
//! directories.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::browse::image_old::{DirentTree, TreePath};
use crate::tui::theme::Palette;
#[allow(unused_imports)]
use crate::tui::browse::image_old::DirentNode;

/// Lines reserved at the bottom for path / count display (not the
/// tree itself — the tree gets the rest of the area).
const FOOTER_LINES: u16 = 0;

/// Render the dirent tree into the given area.
///
/// `visible_indices` is the list of node indices to render, in order.
/// `selected` is the path of the currently selected node. `scroll` is
/// the index of the first visible row.
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    tree: &DirentTree,
    visible_indices: &[usize],
    selected: &TreePath,
    scroll: usize,
    palette: &Palette,
    border_title: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(palette.style_border_focus())
        .title(Span::styled(format!(" {border_title} "), palette.style_title()));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2 + FOOTER_LINES),
    };

    if tree.is_empty() || visible_indices.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            if tree.is_empty() {
                "(no dirents — load image first)"
            } else {
                "no entries (try expanding root)"
            },
            palette.style_dim(),
        )))
        .wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
        return;
    }

    // Compute depth for each visible index so we can indent.
    let items: Vec<ListItem> = visible_indices
        .iter()
        .enumerate()
        .map(|(pos, &idx)| build_row(tree, idx, pos == scroll, &tree.nodes[idx].path == selected, palette))
        .collect();

    // Use ListState to apply scroll offset.
    let mut state = ListState::default();
    state.select(Some(scroll.min(items.len().saturating_sub(1))));
    let list = List::new(items)
        .style(palette.style_base())
        .highlight_style(palette.style_selection());
    frame.render_stateful_widget(list, inner, &mut state);

    // Clear area below inner for any leftover.
    let footer_area = Rect {
        x: area.x + 1,
        y: area.y + 1 + inner.height,
        width: area.width.saturating_sub(2),
        height: FOOTER_LINES,
    };
    if !footer_area.height == 0 {
        frame.render_widget(Clear, footer_area);
    }
}

fn build_row<'a>(
    tree: &DirentTree,
    idx: usize,
    is_at_scroll: bool,
    is_selected: bool,
    palette: &Palette,
) -> ListItem<'a> {
    let node = &tree.nodes[idx];
    let depth = compute_depth(tree, &node.path);
    let indent = "  ".repeat(depth);

    let marker = if tree.is_dir(idx) {
        if is_at_scroll {
            // No special marker — we're just showing it's at the scroll
            // position; rendering highlight via the background style.
            "[+]"
        } else {
            "[+]"
        }
    } else {
        " . "
    };

    let mut style = palette.style_base();
    if node.dirent.is_directory() {
        style = palette.style_dir();
    } else if node.dirent.is_vfat_long() {
        style = palette.style_vfat_long();
    } else if node.dirent.is_file() {
        style = palette.style_archive();
    }

    let display_name = node.dirent.display_name();
    let size = human_bytes(node.size);
    let line = format!("{indent}{marker} {display_name:<16} {size:>10}");

    let line_with_marker = if is_selected {
        Line::from(Span::styled(line, palette.style_selection()))
    } else {
        Line::from(Span::styled(line, style))
    };

    ListItem::new(line_with_marker)
}

/// Compute the depth of a path by counting `/` separators. The root
/// path has depth 0.
fn compute_depth(_tree: &DirentTree, path: &TreePath) -> usize {
    path.as_str().matches('/').count()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_counts_separators() {
        let p = TreePath::root();
        assert_eq!(compute_depth(&DirentTree::default(), &p), 0);
        let q = p.join_dir("A");
        assert_eq!(compute_depth(&DirentTree::default(), &q), 1);
        let r = q.join_dir("B").join("C");
        assert_eq!(compute_depth(&DirentTree::default(), &r), 2);
    }
}
