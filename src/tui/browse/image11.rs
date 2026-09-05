//! 11.x / 12.x image state for the TUI browse mode.
//!
//! Wraps the parser's `ExtractResult` and adds:
//! - filesystem detection by reading each partition's first 512 bytes
//! - cursor position (which partition is selected)
//! - lazy-loading of partition boot sectors (only on demand)

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::ghost11::stream::{ExtractResult, PartitionSummary};

use super::fs_detect::{detect_fs, FsKind};

/// All data the TUI needs to render the 11.x image view.
#[derive(Debug, Clone)]
pub struct Image11State {
    pub source_path: PathBuf,
    pub partitions: Vec<PartitionEntry>,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct PartitionEntry {
    pub summary: PartitionSummary,
    pub fs: FsKind,
}

impl Image11State {
    /// Extract the image to a temporary directory and detect the
    /// filesystem of each partition. Returns an error if the extraction
    /// itself fails (e.g., image is encrypted).
    pub fn load(source_path: PathBuf, extract_result: ExtractResult) -> anyhow::Result<Self> {
        let mut partitions = Vec::with_capacity(extract_result.partitions.len());
        for p in extract_result.partitions {
            let fs = read_first_bytes(&p.output_path, 512)
                .map(|b| detect_fs(&b))
                .unwrap_or(FsKind::Unknown);
            partitions.push(PartitionEntry { summary: p, fs });
        }
        Ok(Self {
            source_path,
            partitions,
            selected: 0,
            scroll: 0,
        })
    }

    /// Selected partition's output path (if any).
    pub fn selected_output_path(&self) -> Option<&PathBuf> {
        self.partitions.get(self.selected).map(|p| &p.summary.output_path)
    }

    pub fn selected(&self) -> Option<&PartitionEntry> {
        self.partitions.get(self.selected)
    }

    /// Move the cursor by `delta`, clamped to the valid range.
    pub fn move_cursor(&mut self, delta: isize) {
        if self.partitions.is_empty() {
            return;
        }
        let len = self.partitions.len() as isize;
        let new = (self.selected as isize + delta).clamp(0, len - 1);
        self.selected = new as usize;
    }

    /// Adjust the scroll so the selected row is visible (called after
    /// `move_cursor`).
    pub fn ensure_visible(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + viewport {
            self.scroll = self.selected + 1 - viewport;
        }
    }
}

fn read_first_bytes(path: &PathBuf, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let read = f.read(&mut buf)?;
    buf.truncate(read);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn summary(index: usize, compressed: u64, decompressed: u64, mbr_type: Option<u8>) -> PartitionSummary {
        PartitionSummary {
            index,
            mbr_type,
            compressed_bytes: compressed,
            decompressed_bytes: decompressed,
            output_path: PathBuf::from(format!("/tmp/p{index}.img")),
        }
    }

    fn state_two_partitions() -> Image11State {
        Image11State {
            source_path: PathBuf::from("/tmp/test.gho"),
            partitions: vec![
                PartitionEntry { summary: summary(0, 1000, 4096, Some(0x07)), fs: FsKind::Ntfs },
                PartitionEntry { summary: summary(1, 50, 256, Some(0x82)), fs: FsKind::Swap },
            ],
            selected: 0,
            scroll: 0,
        }
    }

    #[test]
    fn cursor_move_clamps() {
        let mut s = state_two_partitions();
        s.move_cursor(-5);
        assert_eq!(s.selected, 0, "should not underflow");
        s.move_cursor(100);
        assert_eq!(s.selected, 1, "should clamp to last");
        s.move_cursor(-100);
        assert_eq!(s.selected, 0, "should clamp to first");
    }

    #[test]
    fn ensure_visible_keeps_selected_on_screen() {
        let mut s = state_two_partitions();
        s.selected = 1;
        s.ensure_visible(2);
        assert_eq!(s.scroll, 0, "selected fits without scroll");
    }

    #[test]
    fn empty_state_handles_cursor_moves() {
        let mut s = Image11State {
            source_path: PathBuf::from("/x"),
            partitions: vec![],
            selected: 0,
            scroll: 0,
        };
        s.move_cursor(5);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn read_first_bytes_returns_short_on_eof() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("short.bin");
        std::fs::write(&path, b"abc").unwrap();
        let bytes = read_first_bytes(&path, 512).unwrap();
        assert_eq!(bytes, b"abc");
    }
}
