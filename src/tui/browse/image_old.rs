//! Pre-11.x image state placeholder.
//!
//! Implementation lands in Week 3 (dirent tree builder + tree widget).
//! For Week 2 we only declare the module so the file tree stays stable
//! across commits.

use std::path::PathBuf;

/// Pre-11-x image state — to be filled in Week 3.
#[derive(Debug, Clone)]
pub struct ImageOldState {
    pub source_path: PathBuf,
}

impl ImageOldState {
    pub fn new(source_path: PathBuf) -> Self {
        Self { source_path }
    }
}
