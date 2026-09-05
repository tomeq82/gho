# `gho` — Pure-Rust extractor for Norton Ghost .GHO/.GHS disk images

[![CI](https://github.com/tomeq82/gho/actions/workflows/ci.yml/badge.svg)](https://github.com/tomeq82/gho/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-blue.svg)](https://www.rust-lang.org)

`gho` reads and extracts files from **Norton Ghost** disk image files
(`.gho`, `.ghs`) — the closed-source backup format produced by Symantec /
Broadcom Ghost 11.x, 12.x and earlier versions. Both format families are
supported:

- **Ghost 11.x / 12.x** — partition records, optional FastLZ (Z1) or zlib
  compression, multi-volume span support.
- **pre-11.x** — FAT-style directory of 8.3 dirents with FastLZ (Z1)
  compressed data blocks. (Yes, your 2001 ThinkPad backup counts.)

Both formats were reverse-engineered from the binary record layout of
Norton Ghost 11.5.1. The full specification lives in [`docs/FORMAT.md`](docs/FORMAT.md)
and [`docs/FORMAT_OLD.md`](docs/FORMAT_OLD.md).

## Status

**v0.1.0 — early development.** FastLZ decoder and MBR / dirent parsers are
in place and tested. Full `info` / `extract` / `verify` / `bench` CLI is the
next milestone. See [the planning discussion](https://github.com/tomeq82/gho/issues/1)
for the roadmap.

## Installation

Pre-built binaries for Linux, macOS, and Windows (amd64 + arm64) will land
with the v0.1.0 release. Until then, build from source:

```bash
cargo install --git https://github.com/tomeq82/gho
```

Or grab a Docker image:

```bash
docker run --rm -v "$PWD:/data" ghcr.io/tomeq82/gho:latest info /data/backup.gho
```

## Usage (planned)

```bash
# Inspect an image: format version, partitions, MBR entries
gho info backup.gho

# Extract every partition to ./out/ as raw .img files (mount with `losetup`)
gho extract backup.gho --out ./out/

# Extract just partition N
gho extract backup.gho --out ./out/ --partition 1

# For spanned images, list files in order
gho extract backup.gho backup.GHS backup.GHS --out ./out/

# Verify: walk every record, decompress every block
gho verify backup.gho

# Bench: FastLZ + parser throughput
gho bench backup.gho
```

For pre-11.x images, `extract` walks the directory and supports
`--pattern` to filter by 8.3 substring (e.g. `--pattern MESSAGES.TBB`).

## What this tool does NOT do

- **Encrypted images** are rejected with a clear error. The Ghost
  encryption format is not (yet) reverse-engineered.
- **Mount-as-filesystem** — `gho` extracts to disk. To mount a partition
  image, use `losetup -fP partition_0.img` (Linux) or `hdiutil attach`
  (macOS).
- **Other backup tools** — Acronis TrueImage, Clonezilla, etc. have
  different formats. See [related projects](docs/KNOWN_LIMITATIONS.md).

## Building from source

```bash
git clone https://github.com/tomeq82/gho
cd gho
cargo build --release
./target/release/gho --version
```

Requires Rust 1.85 or newer.

## License

MIT — see [LICENSE](LICENSE).
