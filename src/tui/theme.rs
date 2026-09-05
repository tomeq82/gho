//! Colour palettes for the TUI.
//!
//! Two themes are provided: [`Theme::Dark`] (default) and [`Theme::Light`].
//! Toggle with the `t` key at runtime. Custom themes can be constructed
//! via [`Theme::from_palette`] for tests or user preferences.
//!
//! Palette semantics:
//! - `border_focus`: outline colour for the panel that has keyboard focus
//! - `border_blur`:  outline colour for the inactive panel
//! - `modified`/`added`/`removed`: snapshot-diff highlight colours
//! - `dir`, `file`, `archive`: dirent attribute hints (pre-11.x view)

use ratatui::style::{Color, Style};

/// A user-selectable colour palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Theme::Dark => Palette {
                bg: Color::Reset,
                fg: Color::Reset,
                border_focus: Color::Cyan,
                border_blur: Color::DarkGray,
                title: Color::White,
                selection_bg: Color::Rgb(40, 44, 52),
                selection_fg: Color::White,
                status_bg: Color::Rgb(30, 30, 30),
                status_fg: Color::Gray,
                modified: Color::Yellow,
                added: Color::Green,
                removed: Color::Red,
                dim: Color::DarkGray,
                dir: Color::Blue,
                file: Color::Reset,
                archive: Color::Green,
                vfat_long: Color::Magenta,
                error: Color::Red,
                warn: Color::Yellow,
                ok: Color::Green,
            },
            Theme::Light => Palette {
                bg: Color::Reset,
                fg: Color::Reset,
                border_focus: Color::Blue,
                border_blur: Color::Gray,
                title: Color::Black,
                selection_bg: Color::Rgb(220, 230, 240),
                selection_fg: Color::Black,
                status_bg: Color::Rgb(230, 230, 230),
                status_fg: Color::DarkGray,
                modified: Color::Rgb(180, 130, 0),
                added: Color::Rgb(0, 130, 0),
                removed: Color::Rgb(180, 0, 0),
                dim: Color::Gray,
                dir: Color::Blue,
                file: Color::Reset,
                archive: Color::Rgb(0, 130, 0),
                vfat_long: Color::Magenta,
                error: Color::Red,
                warn: Color::Rgb(180, 130, 0),
                ok: Color::Rgb(0, 130, 0),
            },
        }
    }
}

/// Concrete colour values resolved from a [`Theme`].
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub border_focus: Color,
    pub border_blur: Color,
    pub title: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub modified: Color,
    pub added: Color,
    pub removed: Color,
    pub dim: Color,
    pub dir: Color,
    pub file: Color,
    pub archive: Color,
    pub vfat_long: Color,
    pub error: Color,
    pub warn: Color,
    pub ok: Color,
}

impl Palette {
    pub fn style_base(&self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }

    pub fn style_title(&self) -> Style {
        self.style_base().fg(self.title)
    }

    pub fn style_border_focus(&self) -> Style {
        self.style_base().fg(self.border_focus)
    }

    pub fn style_border_blur(&self) -> Style {
        self.style_base().fg(self.border_blur)
    }

    pub fn style_selection(&self) -> Style {
        self.style_base().bg(self.selection_bg).fg(self.selection_fg)
    }

    pub fn style_modified(&self) -> Style {
        self.style_base().fg(self.modified)
    }

    pub fn style_added(&self) -> Style {
        self.style_base().fg(self.added)
    }

    pub fn style_removed(&self) -> Style {
        self.style_base().fg(self.removed)
    }

    pub fn style_dim(&self) -> Style {
        self.style_base().fg(self.dim)
    }

    pub fn style_status(&self) -> Style {
        self.style_base().bg(self.status_bg).fg(self.status_fg)
    }

    pub fn style_dir(&self) -> Style {
        self.style_base().fg(self.dir)
    }

    pub fn style_archive(&self) -> Style {
        self.style_base().fg(self.archive)
    }

    pub fn style_vfat_long(&self) -> Style {
        self.style_base().fg(self.vfat_long)
    }

    pub fn style_error(&self) -> Style {
        self.style_base().fg(self.error)
    }

    pub fn style_warn(&self) -> Style {
        self.style_base().fg(self.warn)
    }

    pub fn style_ok(&self) -> Style {
        self.style_base().fg(self.ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_round_trip() {
        assert_eq!(Theme::Dark.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Dark);
        assert_eq!(Theme::Dark.next().next(), Theme::Dark);
    }

    #[test]
    fn default_is_dark() {
        assert_eq!(Theme::default(), Theme::Dark);
    }

    #[test]
    fn palettes_have_distinct_colours() {
        // Both themes must define every colour, even if some are the same.
        let dark = Theme::Dark.palette();
        let light = Theme::Light.palette();
        // Sanity: at least selection_bg should differ between themes.
        assert_ne!(format!("{:?}", dark.selection_bg), format!("{:?}", light.selection_bg));
    }
}
