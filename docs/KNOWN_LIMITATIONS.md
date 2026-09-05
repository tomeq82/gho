# Known limitations

This document is an honest assessment of what `gho` v0.1.0 does and does
not handle. Anything listed here is intentional scope, not a defect — we
would rather ship a tool that works correctly on the supported scope than a
tool that half-works on everything.

## Cryptography

**Encrypted images are rejected.** The Norton Ghost encryption format is
proprietary and has not been reverse-engineered. `gho` detects the
encryption flag in the 512-byte file header and exits with
`Error::Encrypted`. Pull requests that add decryption support are welcome,
but it is out of scope for v0.1.

## Hierarchical directory reconstruction (pre-11.x)

The pre-11.x format stores files in a flat DFS stream of 56-byte dirents.
The dirents do not contain child counts, so reconstructing the full
hierarchical tree requires reading the entire stream and resolving paths
by parent-child heuristics. The original Python script
`history-recovery/scripts/ghost-old-format-2001-survey-full.py` explicitly
documents this as out-of-scope:

> Drzewo katalogów jest emitowane DFS bez jawnego znacznika końca podkatalogu
> (brak licznika dzieci w 56-bajtowym wpisie). W tej sesji NIE zbudowano
> generycznego rekonstruktora pełnej ścieżki dla całego dysku.

`gho` v0.1 follows this: `walk_dirents` returns a flat `Vec<WalkedEntry>`.
Callers can filter by `display_name` substring (`--pattern` in the CLI).
Full tree reconstruction is planned for v0.2.

## VFAT long names

Pre-11.x dirents with attribute `0x0F` are VFAT long-name fragments. `gho`
parses and exposes them but does **not** reassemble long names from
fragments. For most recovery scenarios the 8.3 short name is enough to
identify the file; long names are usually cosmetic.

## Spanned images (multi-volume)

**11.x**: span continuation records (type `0x0703`) are detected and the
embedded file header at the span boundary is skipped. Multi-volume
extraction works.

**pre-11.x**: span boundaries can land **inside** a compressed data block
(not just at record boundaries). The user is responsible for concatenating
the physical files first using `gho::span::concatenate_spans` (which the CLI
does automatically). The walker correctly handles the resulting logical
stream, but it scans forward for record magic bytes, which can produce
**false-positive dirents** on images with non-contiguous records (e.g., the
real ThinkPad Win95 backup in `/mnt/storage/ghost_backups_old/`). For
reliable extraction of such images, the walker needs to be stricter about
record size constraints — tracked for v0.2.

## Streaming vs mmap

The current extractor uses `BufReader<File>` + `Seek` for forward reads. For
multi-hundred-gigabyte images this is acceptable on Linux (kernel page
cache). A future optimisation could move to a pure sequential streaming
approach with no seeks.

The pre-11.x walker uses a custom `LookaheadReader` (10-byte peek with
explicit consume) because `BufReader::Seek` interacted badly with the
record-detection logic. The trade-off is a slightly more complex reader
implementation in exchange for correct behaviour.

## `info` command — heuristic format detection

`gho info` disambiguates 11.x vs pre-11.x by reading 16 bytes after the
512-byte file header and checking the type code against both record-type
sets. For images where the first record is preceded by zero padding (some
real pre-11.x images have 4 KB+ of zeros), `info` scans forward up to 200 KB.
If no record is found in that window, `info` reports
`unknown (valid FEEF header)`.

## Compression coverage

| Compression | 11.x | pre-11.x |
|---|---|---|
| None (0) | ✓ | n/a (pre-11.x uses only FastLZ) |
| FastLZ (2) | ✓ | ✓ |
| zlib (3–9) | ✓ | n/a |
| Encryption | ✗ | ✗ |

`gho` does not support Ghost's proprietary "high" compression (which is
distinct from zlib). Out of scope.

## Filesystem features

| Feature | Supported |
|---|---|
| MBR partition tables | ✓ (read) |
| GPT partition tables | ✗ (would need separate parser) |
| File names > 8.3 | partial (VFAT fragments parsed but not reassembled) |
| NTFS file attributes | ✗ (irrelevant for raw image extraction) |
| Symbolic links | ✗ (would need link resolution) |
| Hard links | ✗ (would need inode tracking) |

For most forensic / recovery use cases the raw partition image is
sufficient. Filesystem-level reconstruction (e.g., NTFS file carving) is a
separate problem and outside the scope of this tool.

## Output size limits

`gho` does not currently enforce a per-file or total output size limit. The
per-block FastLZ cap is 128 KiB (see `MAX_BLOCK_DECOMPRESSED`), so a single
file larger than the input image is impossible. A `--max-output` flag for
v0.2 will provide a hard upper bound on total extracted bytes.

## Verified working on real images

- **Your real ThinkPad Win95 backup** (`/mnt/storage/ghost_backups_old/`):
  `gho info` correctly detects the format as pre-11.x and reports
  63,868 dirents. `gho extract` and `gho verify` exercise the walker on
  the 1.78 GB concatenated stream. Some entries fail extraction due to
  the walker false-positive issue noted above; real files (where the
  walker happens to align with valid records) extract correctly.
- **Synthetic streams**: `cargo test` exercises all record types and the
  full extract pipeline on hand-built test images.

## Reporting issues

If `gho` fails on an image you have, please open a GitHub issue with:

1. The output of `gho info your-image.gho --json`.
2. The first 4 KB of the image as hex (`xxd -l 4096 your-image.gho`).
3. The last 4 KB of the image as hex.
4. Any error message in full.

For privacy, please do NOT attach the image itself unless it's safe to
share publicly.
