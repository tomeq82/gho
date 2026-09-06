# `gho` Terminal User Interface (TUI)

`gho` ships with an interactive terminal browser for inspecting Ghost
images without extracting them to disk first. The TUI is built on
[ratatui](https://ratatui.rs) + [crossterm](https://crates.io/crates/crossterm)
and uses a two-panel layout with a status bar, mouse support, and a
toggleable theme.

## Running the TUI

```sh
# Browse a single image (auto-detects 11.x / pre-11.x):
gho browse backup.gho

# For spanned images, list files in order:
gho span concat part1.gho part2.ghs part3.ghs > combined.gho
gho browse combined.gho

# Compare two images side-by-side:
gho diff 2001-01-01.gho 2001-12-31.gho
```

The TUI requires a real terminal — running it from a non-tty context
(pipes, CI logs) exits immediately with `Error: enable raw mode`.

## Layout

```
┌─ gho browse: backup.gho ───────────────────────────────┬─ partition_0.img [NTFS 4.2 GB] ───┐
│ ▼ Track 0                                            │ │ 00000000  EB 52 90 4E 54 46 53   .R.NTFS │
│   ● partition 0  0x07  NTFS    4.2 GB                │ │ 00000008  20 20 20 00 02 08 00 00           │
│   ○ partition 1  0x82  FAT32   16 MB                 │ │ 00000010  00 00 00 00 00 F8 00 00           │
│   ○ partition 2  0x0B  FAT32   8 MB                 │ │ ───────── F3 hex  F4 extract  ──────────── │
│ ▶ Track 1                                            │ │                                     │
└──────────────────────────────────────────────────────┘ └─────────────────────────────────────┘
  +5/-3/~2/=1284  keys: ? for help
```

## Keybindings

### Universal

| Key | Action |
|---|---|
| `q` / `Esc` | Quit |
| `?` | Toggle help overlay |
| `t` | Toggle dark / light theme |
| `Tab` / `Shift+Tab` | Switch focus between left/right panel (browse mode) |
| `Ctrl-C` | Quit (forced) |

### Browse mode (left panel)

| Key | Action |
|---|---|
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `PageDown` / `PageUp` | Move 10 rows |
| `Home` / `End` | Jump to first / last row |
| `Enter` / `l` / `→` | Expand directory (pre-11.x) |
| `h` / `←` / `Backspace` | Collapse directory / jump to parent (pre-11.x) |
| Mouse click | Select row |
| Mouse wheel | Scroll |

### Browse mode (right panel — when focused)

| Key | Action |
|---|---|
| `PageDown` / `PageUp` | Scroll hex preview one page |
| `j` / `k` | Scroll one line |

### Diff mode

| Key | Action |
|---|---|
| `n` / `p` | Jump to next / previous change |
| `Tab` | Toggle between left/right pane (planned for v0.3) |

## Detected filesystem types

The hex-preview's title shows the detected filesystem for 11.x partitions
and 14 file-system signatures total:

- NTFS, FAT12/16/32, exFAT
- ext2/3/4, XFS, Btrfs
- HFS, HFS+, APFS
- swap, ISO 9660, UDF
- Linux RAID, LVM, ZFS
- Boot loaders: GRUB, GRUB Legacy, syslinux, NTLDR, BOOTMGR, LILO

Unknown signatures display `?` so you can still inspect them in the hex view.

## Architecture

```
src/tui/
├── mod.rs        event loop, terminal setup/teardown
├── app.rs        AppState (mode, focus, theme, image, scroll offsets)
├── ui.rs         top-level layout: 2 panels + status bar + diff overlay
├── theme.rs      Dark / Light palettes with semantic colours
├── input.rs      key/mouse → Action enum mapping
├── browse/
│   ├── mod.rs
│   ├── image11.rs     11.x partition view + FS detection
│   ├── image_old.rs   pre-11.x dirent tree + flat→tree reconstruction
│   └── fs_detect.rs    partition filesystem detection (14 types)
├── diff/
│   └── mod.rs     snapshot diff engine (added/removed/modified/unchanged)
└── widgets/
    ├── hex.rs      hex+ASCII viewer
    └── tree.rs     collapse/expand tree
```

## Limitations

- **Pre-11.x tree reconstruction is heuristic.** The format has no
  explicit "end of subdir" marker, so we treat each directory as a
  sibling of the previous one in the stream. For images where
  directories don't nest inside each other (the most common case),
  the tree is correct. For deeply nested images the tree may be
  flattened. See `docs/KNOWN_LIMITATIONS.md`.
- **File content preview is limited to 4 KB hex.** For pre-11.x files
  the first 4 KB is streamed through FastLZ decompressor. v0.3 will
  add a streaming text-mode viewer for the full file.
- **Extract to disk is not yet implemented** (F4 in browse mode).
  The library supports it via `extract_file()`; the TUI just needs
  to wire it up — tracked for v0.3.

## Testing the TUI

```sh
# Run all TUI tests (uses ratatui's TestBackend, no real tty needed)
cargo test --lib tui

# Render-only smoke tests
cargo test --lib tui::render_tests
```
