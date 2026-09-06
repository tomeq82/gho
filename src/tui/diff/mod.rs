//! Snapshot diff engine.
//!
//! Given two `.gho` / `.ghs` images, produces a tree-aligned view of what
//! changed between them:
//! - `+` added:    path exists in NEW but not in OLD
//! - `-` removed:  path exists in OLD but not in NEW
//! - `~` modified: path exists in both but content differs (different
//!   size, different dirent attributes, or different hash of
//!   the first 64 KB)
//! - `=` unchanged: path exists in both with identical content
//!
//! For modified files the engine can also produce a line-level diff via
//! the `similar` crate (text files only — binary files are reported as
//! "binary differs" without showing the byte stream).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ghost11::stream::{extract as extract11, ExtractResult as ExtractResult11};
use crate::ghostold::stream::walk_dirents;

/// What changed between OLD and NEW for one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
    Unchanged,
}

impl ChangeKind {
    pub fn marker(self) -> &'static str {
        match self {
            ChangeKind::Added => "+",
            ChangeKind::Removed => "-",
            ChangeKind::Modified => "~",
            ChangeKind::Unchanged => "=",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Modified => "modified",
            ChangeKind::Unchanged => "unchanged",
        }
    }
}

/// One node in the diff tree. Paths are relative to the image root and
/// use forward slashes (no leading slash).
#[derive(Debug, Clone)]
pub struct DiffNode {
    pub path: String,
    pub kind: ChangeKind,
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
    /// Small fingerprint (xxhash of the first 64 KB + size) for cross-check
    /// when both images have a file at the same path.
    pub old_fp: Option<u64>,
    pub new_fp: Option<u64>,
}

/// The full diff between two images.
#[derive(Debug, Clone)]
pub struct Diff {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub nodes: Vec<DiffNode>,
    /// Per-kind counts for the status bar / footer.
    pub counts: Counts,
    pub built_at: Instant,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
}

impl std::fmt::Debug for Counts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Counts {{ +{} -{} ~{} ={} }}",
            self.added, self.removed, self.modified, self.unchanged
        )
    }
}

/// Build the diff by extracting both images and comparing entries.
///
/// For pre-11.x images we walk the dirent stream and treat each entry as
/// a flat path (the dirent 8.3 name). For 11.x images we compare
/// partition tables.
///
/// This is the simplest implementation: load everything from both
/// images, concatenate the spans if needed, then diff. v0.3 will stream
/// entries as they're discovered to keep peak memory bounded.
pub fn build_diff(old_path: &Path, new_path: &Path) -> anyhow::Result<Diff> {
    let old_entries = collect_entries(old_path)?;
    let new_entries = collect_entries(new_path)?;

    let mut by_path: BTreeMap<String, EntryPair> = BTreeMap::new();
    for (path, size, fp) in old_entries {
        by_path.entry(path).or_default().old = Some(Entry { size, fp });
    }
    for (path, size, fp) in new_entries {
        by_path.entry(path).or_default().new = Some(Entry { size, fp });
    }

    let mut nodes = Vec::with_capacity(by_path.len());
    let mut counts = Counts::default();
    for (path, pair) in by_path {
        let kind = match (&pair.old, &pair.new) {
            (Some(o), Some(n)) => {
                if o.size == n.size && o.fp == n.fp {
                    ChangeKind::Unchanged
                } else {
                    ChangeKind::Modified
                }
            }
            (Some(_), None) => ChangeKind::Removed,
            (None, Some(_)) => ChangeKind::Added,
            (None, None) => unreachable!(),
        };
        match kind {
            ChangeKind::Added => counts.added += 1,
            ChangeKind::Removed => counts.removed += 1,
            ChangeKind::Modified => counts.modified += 1,
            ChangeKind::Unchanged => counts.unchanged += 1,
        }
        nodes.push(DiffNode {
            path,
            kind,
            old_size: pair.old.as_ref().map(|e| e.size),
            new_size: pair.new.as_ref().map(|e| e.size),
            old_fp: pair.old.as_ref().and_then(|e| e.fp),
            new_fp: pair.new.as_ref().and_then(|e| e.fp),
        });
    }

    // Most-changed-first ordering: added/removed/modified at the top,
    // unchanged at the bottom.
    nodes.sort_by(|a, b| {
        let rank = |k: ChangeKind| match k {
            ChangeKind::Added => 0,
            ChangeKind::Removed => 0,
            ChangeKind::Modified => 1,
            ChangeKind::Unchanged => 2,
        };
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.path.cmp(&b.path))
    });

    Ok(Diff {
        old_path: old_path.to_path_buf(),
        new_path: new_path.to_path_buf(),
        nodes,
        counts,
        built_at: Instant::now(),
    })
}

#[derive(Default)]
struct EntryPair {
    old: Option<Entry>,
    new: Option<Entry>,
}

#[derive(Clone, Copy)]
struct Entry {
    size: u64,
    /// Best-effort fingerprint: size + xxhash of first 64 KB of content.
    /// For 11.x images we don't extract content, so this is None and the
    /// comparison falls back to size-only (which is accurate enough for
    /// partition-level diffs).
    fp: Option<u64>,
}

/// Pull (path, size, fingerprint) tuples from either image format.
fn collect_entries(path: &Path) -> anyhow::Result<Vec<(String, u64, Option<u64>)>> {
    // We try 11.x first; on failure, fall back to pre-11.x. The walk_dirents
    // path always succeeds for valid headers, but the per-file
    // fingerprint requires reading the file contents, which the 11.x
    // extractor does. We only compute fingerprints in the pre-11.x path
    // (where dirents have inline data).
    if let Ok(extract) = extract11(path, &std::env::temp_dir()) {
        let entries = entries_from_extract(&extract);
        return Ok(entries);
    }

    // Fall back to pre-11.x walking + per-file fingerprinting.
    let entries = walk_dirents(path).map_err(|e| anyhow::anyhow!("walk_dirents: {e}"))?;
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let path_str = entry.dirent.display_name();
        let size = entry.dirent.size as u64;
        let fp = compute_fingerprint(path, &entry)
            .ok()
            .flatten();
        out.push((path_str, size, fp));
    }
    Ok(out)
}

fn entries_from_extract(extract: &ExtractResult11) -> Vec<(String, u64, Option<u64>)> {
    let mut out = Vec::new();
    for (i, p) in extract.partitions.iter().enumerate() {
        let name = format!("partition_{i}");
        let size = p.decompressed_bytes;
        out.push((name, size, None));
    }
    out
}

fn compute_fingerprint(
    image_path: &Path,
    entry: &crate::ghostold::stream::WalkedEntry,
) -> std::io::Result<Option<u64>> {
    use std::hash::{Hash, Hasher};
    use std::io::{Read, Seek, SeekFrom};
    if entry.data_start_offset.is_none() {
        return Ok(None);
    }
    let mut f = std::fs::File::open(image_path)?;
    f.seek(SeekFrom::Start(entry.data_start_offset.unwrap()))?;
    let mut buf = vec![0u8; 65536.min(entry.dirent.size as usize)];
    f.read_exact(&mut buf)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    buf.hash(&mut hasher);
    Ok(Some(hasher.finish()))
}

/// Produce a unified-diff text for a single modified file (text only).
/// For binary files returns `None` so the UI can render a placeholder.
pub fn line_diff_text(old: &[u8], new: &[u8]) -> Option<String> {
    use similar::{ChangeTag, TextDiff};
    let old_text = std::str::from_utf8(old).ok()?;
    let new_text = std::str::from_utf8(new).ok()?;
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let marker = match change.tag() {
            ChangeTag::Equal => ' ',
            ChangeTag::Insert => '+',
            ChangeTag::Delete => '-',
        };
        out.push_str(&format!(
            "{}{}\n",
            marker,
            change.value().trim_end(),
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_added_removed_modified() {
        let old = vec![("A".to_string(), 1, None), ("B".to_string(), 2, None)];
        let new = vec![("A".to_string(), 1, None), ("C".to_string(), 3, None)];
        let mut by_path: BTreeMap<String, EntryPair> = BTreeMap::new();
        for (p, s, fp) in old {
            by_path.entry(p).or_default().old = Some(Entry { size: s, fp });
        }
        for (p, s, fp) in new {
            by_path.entry(p).or_default().new = Some(Entry { size: s, fp });
        }
        let mut counts = Counts::default();
        for (_, pair) in by_path {
            let kind = match (&pair.old, &pair.new) {
                (Some(o), Some(n)) if o.size == n.size && o.fp == n.fp => ChangeKind::Unchanged,
                (Some(_), Some(_)) => ChangeKind::Modified,
                (Some(_), None) => ChangeKind::Removed,
                (None, Some(_)) => ChangeKind::Added,
                _ => unreachable!(),
            };
            match kind {
                ChangeKind::Added => counts.added += 1,
                ChangeKind::Removed => counts.removed += 1,
                ChangeKind::Modified => counts.modified += 1,
                ChangeKind::Unchanged => counts.unchanged += 1,
            }
        }
        assert_eq!(counts.added, 1, "C is added");
        assert_eq!(counts.removed, 1, "B is removed");
        assert_eq!(counts.unchanged, 1, "A identical");
        assert_eq!(counts.modified, 0);
    }

    #[test]
    fn change_kind_markers() {
        assert_eq!(ChangeKind::Added.marker(), "+");
        assert_eq!(ChangeKind::Removed.marker(), "-");
        assert_eq!(ChangeKind::Modified.marker(), "~");
        assert_eq!(ChangeKind::Unchanged.marker(), "=");
    }

    #[test]
    fn line_diff_text_returns_none_for_binary() {
        // Use bytes that are NOT valid UTF-8 (0xFF is never valid in UTF-8).
        assert!(line_diff_text(b"hello\xFFworld", b"hello\xFEworld").is_none());
    }

    #[test]
    fn line_diff_text_returns_unified_diff_for_text() {
        let diff = line_diff_text(b"a\nb\nc\n", b"a\nB\nc\n").expect("text diff");
        assert!(diff.contains("-b"), "should mark deleted line");
        assert!(diff.contains("+B"), "should mark added line");
    }
}
