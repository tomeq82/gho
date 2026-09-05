//! `gho` — Pure-Rust extractor for Norton Ghost .GHO/.GHS disk images.
//!
//! Supports two format families:
//! - **11.x / 12.x**: FEEF magic, partition records, FastLZ (Z1) or zlib compression.
//! - **pre-11.x**: FEEF magic, FAT-style directory of 8.3 dirents with FastLZ (Z1) blocks.
//!
//! See `docs/FORMAT.md` and `docs/FORMAT_OLD.md` for the format specifications
//! derived from reverse-engineering of Norton Ghost 11.5.1.

pub mod error;
pub mod fastlz;
pub mod format;
pub mod ghost11;
pub mod ghostold;
pub mod mbr;
pub mod safety;
pub mod span;

pub use error::{Error, Result};
pub use format::{Format, ImageReader};
