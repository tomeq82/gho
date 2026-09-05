//! `gho browse` mode — single-image TUI browser.
//!
//! This module is split across several files for clarity:
//! - `fs_detect` — partition filesystem identification by magic bytes
//! - `image11` — 11.x / 12.x image loading + partition list
//! - `image_old` — pre-11.x dirent tree (Week 3)

pub mod fs_detect;
pub mod image11;
pub mod image_old;

/// One of the two image format families, normalised for the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// Norton Ghost 11.x or 12.x (FEEF magic + partition records).
    Ghost11,
    /// Norton Ghost pre-11.x (FEEF magic + dirent stream).
    GhostOld,
    /// Image could not be classified — header was valid but format was not
    /// detected, or extraction failed.
    Unknown,
}
