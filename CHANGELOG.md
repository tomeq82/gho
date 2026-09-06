# Changelog

All notable changes to `gho` are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-09-06

### Added
- **Interactive TUI browser** (`gho browse <image>`):
  - Two-panel layout with status bar, mouse capture, dark/light theme
    toggle, help overlay.
  - 11.x partition view with filesystem detection (14 FS types plus 6
    boot loaders — NTFS, FAT12/16/32, exFAT, ext2/3/4, XFS, Btrfs, HFS+,
    APFS, swap, ISO 9660, UDF, Linux RAID, LVM, ZFS, GRUB, etc.).
  - Pre-11.x dirent tree with collapse/expand (Enter/Right to expand,
    Left/Backspace to collapse, Up/Down to navigate).
  - Hex+ASCII preview pane (first 4 KB of selected file/partition).
  - See [`docs/TUI.md`](docs/TUI.md) for full keybinding reference.
- **Snapshot diff subcommand** (`gho diff <old> <new>`):
  - Compares two Ghost images: added / removed / modified / unchanged
    classification of every dirent.
  - Line-level unified diff for text files via the `similar` crate.
  - Overlay counts in the status bar (`+5/-3/~2/=1284`).
- New dependencies: `ratatui`, `crossterm`, `similar`.
- New fuzz targets in AFL++ harness for diff / tree / hex widgets (to be
  wired in Week 6).
- 90 new unit tests covering TUI rendering, FS detection, tree
  reconstruction, diff engine, hex widget rendering.

### Changed
- `Dirent::display_name` now trims both `0x20` (space) and `0x00` (NUL)
  padding, and uses a 1-char `?` fallback for invalid UTF-8 so the
  returned length stays within the normal 8.3 budget.

### Limitations (see `docs/KNOWN_LIMITATIONS.md` for full list)
- **Pre-11.x tree reconstruction is heuristic.** The format has no
  explicit "end of subdir" marker; we treat each directory as a sibling
  of the previous one in the DFS stream. For images where directories
  don't nest (the common case), the tree is correct. For deeply nested
  images it may be flattened.
- File extraction from the TUI (F4) is stubbed; the underlying
  `extract_file` works but the streaming-on-keypress wiring lands in
  v0.3.
- Diff metadata for 11.x images only compares partition-level sizes
  (no per-file content fingerprints yet) — pre-11.x diffs hash the
  first 64 KB of each file.

## [0.1.0] - 2026-09-05

### Added
- Pre-11.x format walker (`ghostold::stream::walk_dirents`)
- Pre-11.x single-file extractor (`ghostold::stream::extract_file`)
- `safety` module: path sanitisation (`sanitize_8_3`, `fallback_name`,
  `contains_parent_traversal`)
- `MAX_BLOCK_DECOMPRESSED` cap (128 KiB) on FastLZ output
- Security test suite (`tests/security.rs`, 18 tests)
- Property-based tests (`tests/property.rs`, 6 tests)
- Fuzz harnesses (`fuzz/`) for `fastlz_decompress`, `ghost11_extract`,
  `ghostold_walk`
- CI workflow (`.github/workflows/ci.yml`)
- Release workflow with multi-arch builds, cosign signing, Docker push
  (`.github/workflows/release.yml`)
- Nightly fuzz workflow (`.github/workflows/fuzz.yml`)
- Multi-stage Dockerfile (rust:1.85-slim → distroless static, nonroot)
- `docs/SECURITY.md` (threat model + mitigations index)
- `docs/KNOWN_LIMITATIONS.md` (honest v0.1 scope assessment)

### Changed
- `concatenate_spans` now keeps the first file's header and only strips
  continuation-span headers (matches `history-recovery` Python
  `build_logical`)
- `contains_parent_traversal` no longer flags absolute paths (only `..`
  segments)

### Known limitations
See [`docs/KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md). Summary:
- Encrypted images are rejected (encryption not RE'd)
- Pre-11.x walker has false-positive dirents on real images with
  non-contiguous records (improvement tracked for v0.2)
- Hierarchical directory tree reconstruction not supported
- VFAT long-name fragments are parsed but not reassembled

[Unreleased]: https://github.com/tomeq82/gho/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/tomeq82/gho/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/tomeq82/gho/releases/tag/v0.1.0
