//! Pre-11.x image state for the TUI browse mode.
//!
//! Wraps the parser's flat `WalkedEntry` list and reconstructs a tree
//! using the "nearest preceding directory" heuristic. This is the same
//! heuristic the original `history-recovery` Python tools use, which
//! works well for images that store all subdirectories as flat chains
//! (e.g., The Bat!'s `FOLDER/MESSAGES.TBB`). For more complex directory
//! trees the structure may be wrong; we document this limitation in
//! `docs/KNOWN_LIMITATIONS.md`.
//!
//! The flat list is also exposed so the UI can fall back to it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::ghostold::stream::{walk_dirents, WalkedEntry};
use crate::ghostold::dirent::Dirent;

/// All data the TUI needs to render the pre-11.x image view.
#[derive(Debug, Clone)]
pub struct ImageOldState {
    pub source_path: PathBuf,
    pub tree: DirentTree,
    pub selected: TreePath,
    pub scroll: usize,
    pub expanded: Vec<TreePath>,
    pub last_loaded: Instant,
}

/// A path into the dirent tree — either a directory or a file.
/// Paths use the conventional separator `/`. The root directory is the
/// empty path (`""`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TreePath(pub String);

impl TreePath {
    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Push a name (file or directory) onto the path. Returns the new path.
    pub fn join(&self, name: &str) -> Self {
        let mut s = self.0.clone();
        s.push_str(name);
        Self(s)
    }

    /// Push a directory name onto the path, adding a trailing `/`.
    pub fn join_dir(&self, name: &str) -> Self {
        let mut s = self.0.clone();
        s.push_str(name);
        s.push('/');
        Self(s)
    }

    /// Last component of the path (or "" for root).
    pub fn basename(&self) -> &str {
        self.0.rsplit_once('/').map(|(_, b)| b).unwrap_or(&self.0)
    }

    /// Parent directory path. If this path is `PROG/FILE.EXE`, the parent
    /// is `PROG/` (with trailing slash, so directories stay distinguishable
    /// from files even when their 8.3 names overlap). If this path is
    /// already just `PROG/`, the parent is `""` (root).
    pub fn parent(&self) -> TreePath {
        // Strip the trailing component name. Reconstruct the directory
        // prefix by taking everything up to (but not including) the last
        // slash, then adding the slash back.
        match self.0[..self.0.len().saturating_sub(1)].rsplit_once('/') {
            Some((parent_dir, _file_name)) => {
                let mut s = parent_dir.to_string();
                s.push('/');
                TreePath(s)
            }
            None => TreePath::root(),
        }
    }
}

/// One node in the reconstructed dirent tree.
#[derive(Debug, Clone)]
pub struct DirentNode {
    pub dirent: Dirent,
    pub path: TreePath,
    pub size: u64,
}

/// A flat-to-hierarchical tree: directories store their children in
/// insertion order.
#[derive(Debug, Clone, Default)]
pub struct DirentTree {
    pub nodes: Vec<DirentNode>,
    /// For each node index in `nodes`, indices of its direct children.
    /// For files (leaf nodes), this is empty.
    pub children_of: Vec<Vec<usize>>,
}

impl DirentTree {
    /// Total number of nodes (files + directories) in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Flatten the tree (respecting `expanded` dirs) into an ordered list
    /// of node indices. Directories are emitted before their contents.
    /// Hidden nodes (whose ancestor directory is collapsed) are skipped.
    pub fn visible_indices(&self, expanded: &[TreePath]) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.nodes.len());
        if self.nodes.is_empty() {
            return out;
        }
        // Root's children — root itself is implicit, not in `nodes`.
        // We use the convention: root = node index 0 if it exists, but
        // dirent trees from `walk_dirents` may have a root dirent or not.
        // For simplicity: always emit node 0 if it's a directory, then
        // recurse.
        self.walk(0, expanded, &mut out);
        out
    }

    fn walk(&self, idx: usize, expanded: &[TreePath], out: &mut Vec<usize>) {
        out.push(idx);
        if !self.is_dir(idx) {
            return;
        }
        if !expanded.contains(&self.nodes[idx].path) {
            return;
        }
        for &child in &self.children_of[idx] {
            self.walk(child, expanded, out);
        }
    }

    pub fn is_dir(&self, idx: usize) -> bool {
        self.nodes.get(idx).is_some_and(|n| n.dirent.is_directory())
    }

    /// Look up a node by path.
    pub fn find(&self, path: &TreePath) -> Option<usize> {
        self.nodes.iter().position(|n| &n.path == path)
    }
}

impl ImageOldState {
    pub fn new(source_path: PathBuf) -> Self {
        Self {
            source_path,
            tree: DirentTree::default(),
            selected: TreePath::root(),
            scroll: 0,
            expanded: vec![TreePath::root()],
            last_loaded: Instant::now(),
        }
    }

    /// Load by concatenating the input spans, then walking dirents.
    /// Extraction happens lazily on F4 (per-file). This is fine for
    /// pre-11.x because the dirent metadata is in the header / first
    /// record and doesn't require decompressing payloads.
    pub fn load(&mut self) -> anyhow::Result<()> {
        let entries = walk_dirents(&self.source_path)
            .map_err(|e| anyhow::anyhow!("walk_dirents: {e}"))?;
        self.tree = build_dirent_tree(&entries);
        self.selected = TreePath::root();
        self.scroll = 0;
        self.expanded = vec![TreePath::root()];
        self.last_loaded = Instant::now();
        Ok(())
    }

    /// Expand a directory (show its children).
    pub fn expand(&mut self, path: TreePath) {
        if !self.expanded.contains(&path) {
            self.expanded.push(path);
        }
    }

    /// Collapse a directory (hide its children).
    pub fn collapse(&mut self, path: TreePath) {
        self.expanded.retain(|p| p != &path);
    }

    /// Move selection by `delta` in the visible-node list.
    pub fn move_cursor(&mut self, delta: isize) {
        let visible = self.tree.visible_indices(&self.expanded);
        if visible.is_empty() {
            return;
        }
        let current_pos = visible.iter().position(|&i| self.tree.nodes[i].path == self.selected);
        let new_pos = match current_pos {
            Some(p) => (p as isize + delta).clamp(0, visible.len() as isize - 1) as usize,
            None if delta >= 0 => 0,
            None => visible.len() - 1,
        };
        self.selected = self.tree.nodes[visible[new_pos]].path.clone();
    }

    /// Adjust the scroll so the selected node is in view.
    pub fn ensure_visible(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        let visible = self.tree.visible_indices(&self.expanded);
        if let Some(pos) = visible.iter().position(|&i| self.tree.nodes[i].path == self.selected) {
            if pos < self.scroll {
                self.scroll = pos;
            } else if pos >= self.scroll + viewport {
                self.scroll = pos + 1 - viewport;
            }
        }
    }

    /// Extract a single file/dir to `out_path`. (Stubbed for v0.2 — see
    /// TODO in `docs/TODO.md`; the real implementation needs streaming
    /// decompression + a worker thread to avoid blocking the TUI event loop.)
    pub fn extract_to(&self, _entry: &DirentNode, _out_path: &Path) -> anyhow::Result<u64> {
        anyhow::bail!("extract is not yet implemented in v0.2 (planned for v0.3)")
    }
}

/// Reconstruct a tree from the flat dirent stream.
///
/// Algorithm: walk the entries in order. Treat every directory as a
/// SIBLING of the previous one — i.e. when a new directory appears in
/// the stream, we close out the previous directory's scope (pop it from
/// the parent stack) before pushing the new one. Files emitted after
/// a directory belong to that directory.
///
/// This is the same heuristic the original `history-recovery` Python
/// tools use. It correctly recovers images where each top-level
/// directory's contents are fully emitted before the next top-level
/// directory (e.g., The Bat!'s `FOLDER/MESSAGES.TBB+`.TBI` layout, or
/// any image where directories don't nest inside each other in the
/// DFS stream). For images with deeper nesting, the tree will be
/// flattened — documented as a v0.2 limitation.
pub fn build_dirent_tree(entries: &[WalkedEntry]) -> DirentTree {
    let mut tree = DirentTree {
        nodes: Vec::with_capacity(entries.len()),
        children_of: Vec::with_capacity(entries.len()),
    };
    if entries.is_empty() {
        return tree;
    }

    // Parent stack: starts at root. Before pushing a directory we pop
    // the previous directory's scope (so all top-level directories are
    // siblings under root).
    let mut parent_stack: Vec<(TreePath, Option<usize>)> = vec![(TreePath::root(), None)];

    for entry in entries {
        let d = &entry.dirent;

        if d.is_directory() {
            // Close out the previous directory scope (keep root at
            // the bottom of the stack).
            if parent_stack.len() > 1 {
                parent_stack.pop();
            }
            let (parent_path, parent_idx) = parent_stack.last().unwrap().clone();

            let dir_path = parent_path.join_dir(&d.display_name());
            let node = DirentNode {
                dirent: d.clone(),
                path: dir_path.clone(),
                size: 0,
            };
            tree.nodes.push(node);
            tree.children_of.push(Vec::new());
            let new_idx = tree.nodes.len() - 1;

            if let Some(pidx) = parent_idx {
                tree.children_of[pidx].push(new_idx);
            }
            parent_stack.push((dir_path, Some(new_idx)));
        } else {
            // File → child of the top of stack (most recent directory,
            // or root if no directory has appeared yet).
            let (parent_path, parent_idx) = parent_stack.last().unwrap().clone();
            let file_path = parent_path.join(&d.display_name());
            let node = DirentNode {
                dirent: d.clone(),
                path: file_path,
                size: d.size as u64,
            };
            tree.nodes.push(node);
            tree.children_of.push(Vec::new());
            let new_idx = tree.nodes.len() - 1;

            if let Some(pidx) = parent_idx {
                tree.children_of[pidx].push(new_idx);
            }
        }
    }

    tree
}

#[cfg(test)]
mod tests {
    use super::*;
use crate::ghostold::dirent::Dirent;

    fn fake_dirent(name: &str, ext: &str, attrs: u8, size: u32) -> Dirent {
        let mut buf = [0u8; 56];
        let n = name.as_bytes();
        let e = ext.as_bytes();
        let n_len = n.len().min(8);
        let e_len = e.len().min(3);
        buf[..n_len].copy_from_slice(&n[..n_len]);
        buf[8..8 + e_len].copy_from_slice(&e[..e_len]);
        buf[11] = attrs;
        buf[28..32].copy_from_slice(&size.to_le_bytes());
        Dirent::parse(&buf).unwrap()
    }

    #[test]
    fn tree_path_join_and_parent() {
        let p = TreePath::root();
        let q = p.join_dir("PROGRAM");
        let r = q.join("FILE.EXE");
        assert_eq!(r.as_str(), "PROGRAM/FILE.EXE");
        assert_eq!(r.parent().as_str(), "PROGRAM/");
        assert_eq!(r.parent().parent().as_str(), "");
    }

    #[test]
    fn build_tree_empty() {
        let tree = build_dirent_tree(&[]);
        assert!(tree.is_empty());
    }

    #[test]
    fn build_tree_chains_directories() {
        // Build: <root>/PROG/APP.EXE, <root>/DATA/INFO.TXT
        let entries = vec![
            WalkedEntry {
                dirent_offset: 0,
                dirent: fake_dirent("PROG", "", 0x10, 0),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: true,
            },
            WalkedEntry {
                dirent_offset: 1,
                dirent: fake_dirent("APP", "EXE", 0x20, 1234),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: false,
            },
            WalkedEntry {
                dirent_offset: 2,
                dirent: fake_dirent("DATA", "", 0x10, 0),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: true,
            },
            WalkedEntry {
                dirent_offset: 3,
                dirent: fake_dirent("INFO", "TXT", 0x20, 56),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: false,
            },
        ];
        let tree = build_dirent_tree(&entries);
        assert_eq!(tree.len(), 4);
        for (i, n) in tree.nodes.iter().enumerate() {
            eprintln!("  node {}: path={:?}, is_dir={}, children={:?}", i, n.path.as_str(), tree.is_dir(i), tree.children_of[i]);
        }
        // PROG should contain APP.EXE
        let prog_path = TreePath::root().join_dir("PROG");
        let prog_idx = tree.find(&prog_path).expect("PROG node");
        assert!(tree.is_dir(prog_idx));
        assert_eq!(tree.children_of[prog_idx].len(), 1);
        // The child should be the APP.EXE file
        let app_idx = tree.children_of[prog_idx][0];
        assert_eq!(tree.nodes[app_idx].dirent.display_name(), "APP.EXE");
        assert_eq!(tree.nodes[app_idx].path.as_str(), "PROG/APP.EXE");
        // DATA should contain INFO.TXT
        let data_idx = tree.find(&TreePath::root().join_dir("DATA")).unwrap();
        assert_eq!(tree.children_of[data_idx].len(), 1);
    }

    #[test]
    fn visible_indices_expands_root_only_by_default() {
        let entries = vec![
            WalkedEntry {
                dirent_offset: 0,
                dirent: fake_dirent("PROG", "", 0x10, 0),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: true,
            },
            WalkedEntry {
                dirent_offset: 1,
                dirent: fake_dirent("APP", "EXE", 0x20, 100),
                data_start_offset: None,
                full_block_count: 0,
                last_block_decompressed_size: 0,
                is_empty: false,
            },
        ];
        let tree = build_dirent_tree(&entries);
        // Only root is expanded; PROG should be visible but APP.EXE hidden.
        let visible = tree.visible_indices(&[TreePath::root()]);
        assert_eq!(visible.len(), 1, "only PROG visible, not APP.EXE");
        assert_eq!(tree.nodes[visible[0]].dirent.display_name(), "PROG");

        // Now expand PROG.
        let prog_path = TreePath::root().join_dir("PROG");
        let visible = tree.visible_indices(&[TreePath::root(), prog_path.clone()]);
        assert_eq!(visible.len(), 2);
        assert_eq!(tree.nodes[visible[1]].dirent.display_name(), "APP.EXE");
    }
}
