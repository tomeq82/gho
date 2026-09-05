# Changelog

All notable changes to `gho` are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

## [0.1.0] - 2026-XX-XX

Initial public release. See commit history for the full list of changes.

### Highlights
- Ghost 11.x/12.x and pre-11.x format support
- FastLZ (Z1) decoder with trusted reference vectors
- MBR partition table parser
- 56-byte FAT-style dirent parser
- CLI with `info`, `extract`, `verify`, `bench` subcommands
- Multi-platform: Linux, macOS, Windows (amd64 + arm64)
- Single static binary per target (~6-10 MB)
- Cosign-signed releases + SBOM
- Multi-arch Docker image (linux/amd64 + linux/arm64)

[Unreleased]: https://github.com/tomeq82/gho/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tomeq82/gho/releases/tag/v0.1.0
