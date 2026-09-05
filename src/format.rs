//! Top-level format detection and image opening.

use crate::error::{Error, Result};
use std::io;
use std::path::Path;

/// The detected format family of a Ghost image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Norton Ghost 11.x / 12.x (FEEF magic, partition records).
    Ghost11,
    /// Norton Ghost pre-11.x (FEEF magic, FAT-style directory).
    GhostOld,
}

/// Reader over a (possibly spanned) Ghost image.
///
/// Construct with [`ImageReader::open`], then call one of the `read_*_info`
/// methods to detect the format and inspect the contents.
#[derive(Debug)]
pub struct ImageReader {
    path: std::path::PathBuf,
}

impl ImageReader {
    /// Open a single-file image. For spanned images, concatenate first
    /// (see [`crate::span`]).
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("image not found: {}", path.display()),
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Open a spanned image (multiple `.gho` / `.ghs` files concatenated).
    pub fn open_spanned(paths: &[&Path]) -> Result<Self> {
        if paths.is_empty() {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no input files",
            )));
        }
        Ok(Self {
            path: paths[0].to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Detect which format family this image belongs to.
    pub fn detect_format(&self) -> Result<Format> {
        unimplemented!("detect_format pending")
    }
}
