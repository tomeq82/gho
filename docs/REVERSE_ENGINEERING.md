# Reverse-engineering methodology

This document describes how `gho` was reverse-engineered from the Norton
Ghost 11.5.1 implementation. The output is a portable, dependency-free
pure-Rust library that handles both 11.x and pre-11.x formats.

## Why reverse engineering?

Norton Ghost is closed-source software by Symantec (now Broadcom). The
binary `.gho` and `.ghs` file formats are undocumented. The only public
information comes from:

- Older source code leaks (1990s-era Ghost 4.x / 5.x, which used a
  different file structure)
- The `GhostExplorer` Windows application (closed-source)
- Byte-level analysis of real backups

Forensic recovery scenarios (your ThinkPad backup, archives from old
backups) need a way to extract data without installing the proprietary
tool, especially since Ghost 11.5.1 was a Windows-only application that
no longer installs cleanly on modern systems.

## Phase 1 — Reference encoder

The fastest way to RE a binary format is to find a trustworthy encoder.
`history-recovery/gho create` (in the same family of projects) is a
re-implementation that produces real Ghost-compatible images. We used it to
generate a known-good compressed block:

- Input: `b"ABC" * 240` (720 bytes)
- Output: 93-byte FastLZ block (`KNOWN_FASTLZ_BLOCK`)
- Decompressor under test must produce exactly the same 720 bytes

This 93-byte hex blob is now part of the test suite as
`fastlz::tests::KNOWN_FASTLZ_BLOCK`. Any divergence from this expected
output is a bug in the decoder (or in the encoder, but the encoder is also
in our codebase and tested independently).

## Phase 2 — 11.x / 12.x format

The 11.x format is structured:

- 512-byte file header (`FEEF` magic, file_type, compression, image_id)
- Stream of records with 10-byte headers
- Compressed blocks between records

Once we understood the structure (by reading old Ghost C++ source code
leaks, the format is not encrypted or obfuscated), implementing the
parser was straightforward. The known record types are:

- `0x0006` Track0 (with optional MBR)
- `0x0603` Partition
- `0x0703` Continuation
- `0x0023` End

The FastLZ ("Z1") block decompressor was ported line-by-line from
`history-recovery.ghost_image.fastlz_decompress` (a Python implementation
that itself was a careful port of the C++ original).

The compression format numbers:

- 0 = none
- 2 = FastLZ (Z1)
- 3..=9 = zlib (RFC 1950)

Each partition has `size / 32 KiB` full blocks followed by one partial
block. The exact block size and trailer arrangement was confirmed by
extracting known files from real images and checking SHA-256 against
the original.

## Phase 3 — Pre-11.x format

The pre-11.x format is fundamentally different: instead of partition
records, the image contains a flat directory stream.

The original reverse engineering was done in September 2026 and is
documented in `history-recovery/docs/handover/GHOST_OLD_FORMAT.md`. Key
insights:

1. **Same magic and record header wire format** as 11.x, but different
   type codes (`0x2c17`, `0x2c04`, `0x0104`, `0x0002`, `0x0102`,
   `0x0103`, `0x0118`, `0x0117`).
2. **Dirents are 56-byte FAT-style entries** (8.3 name + extension +
   FAT attributes + cluster + size). The size field is the only one used
   for extraction.
3. **Span boundaries can land inside compressed blocks**, not at record
   boundaries. This is the trickiest part: the entire concatenated
   logical stream has the 512-byte file headers stripped at known file
   boundaries.
4. **The directory tree is emitted DFS without explicit end-of-subdir
   markers** — there's no child count in the 56-byte entry. Reconstructing
   the full tree requires building a parent map; the original
   `history-recovery` Python intentionally skipped this.

## Phase 4 — Test corpus

Real `.gho`/`.ghs` files in the wild:

| Source | Format | Notes |
|---|---|---|
| `history-recovery/.venv-api/lib/...` | (test fixtures) | Tiny synthetic streams built in test code |
| `/mnt/storage/ghost_backups_old/laptopas.gho` + `.GHS` x2 | pre-11.x | Real 2001-era backup from a ThinkPad running Win95 |

The second item is invaluable: 1.78 GB of real data with all the
quirks (zero padding between header and first record, multiple partition
boundaries, etc.). The `tests/real_image.rs` integration tests (marked
`#[ignore]` because they require the real file path) exercise the walker
against this image.

## Phase 5 — Security hardening

A binary format parser is an attack surface. The Rust language prevents
many classes of bug (buffer overflows, use-after-free), but additional
mitigations are needed:

1. **Decompression bombs**: a `MAX_BLOCK_DECOMPRESSED` cap of 128 KiB on
   FastLZ output. Verified by `tests/security.rs::fastlz_compressed_block_bounded_by_output_cap`.
2. **Path traversal**: `safety::sanitize_8_3` neutralises `../`,
   absolute paths, control characters in dirent names before they reach
   the filesystem. Verified by `tests/security.rs::path_traversal_*`.
3. **Resource exhaustion**: dirent and block counts bounded by input
   size, no unbounded in-memory structures.
4. **Fuzzing**: three libFuzzer harnesses (`fastlz_decompress`,
   `ghost11_extract`, `ghostold_walk`) run nightly in CI. Local runs
   hit 600k+ iterations without finding panics.

## Tools used

- `xxd`, `hexdump` for hex inspection
- `python3 -c "..."` for quick format experiments
- `cargo +nightly fuzz run` for property-based testing
- `git` + `forgejo` (private) for tracking the reverse-engineering work
- `gh` CLI for GitHub interactions
- Lots of `printf` and `dd` for crafting test inputs

## References

- `history-recovery/docs/handover/GHOST_OLD_FORMAT.md`
- `history-recovery/scripts/ghost-old-format-2001-*`
- `history-recovery/history_recovery/ghost_image.py` (Python reference)
- [`docs/FORMAT.md`](FORMAT.md)
- [`docs/FORMAT_OLD.md`](FORMAT_OLD.md)
- [`docs/SECURITY.md`](SECURITY.md)
- [`docs/KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md)
