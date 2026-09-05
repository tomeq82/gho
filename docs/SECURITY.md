# Security

`gho` reads untrusted binary data (`.gho`/`.ghs` files) from disk and writes
extracted content (partitions, files) to a user-specified output directory.
This document describes the threat model, mitigations, and the security test
suite that enforces them.

## Threat model

### Adversary capabilities

- **Crafted image**: an attacker can supply any byte sequence as a `.gho` file
  (e.g., downloaded from the Internet, attached to a bug report, shared by
  a third party).
- **Path control**: in the pre-11.x format, file names come from FAT-style
  8.3 dirent fields. An attacker who controls the image controls these names.
- **Local execution**: `gho` runs with the user's privileges. The output
  directory is whatever the user specified.

### Adversary goals

1. **Code execution** via memory corruption (buffer overflow, use-after-free,
   integer overflow) in the parser or decompressor.
2. **Path traversal / arbitrary file write**: e.g. extract a file named
   `../../.ssh/authorized_keys` into the user's home directory.
3. **Resource exhaustion**: trick the tool into allocating gigabytes of RAM,
   filling the output disk, or running forever.
4. **Information disclosure**: leak data from one extraction into another
   via symlink attacks on the output directory.

## Mitigations

### Memory safety

The Rust compiler enforces memory safety at compile time:

- Slice and `Vec` indexing is bounds-checked; no buffer overflows.
- Ownership and borrowing prevent use-after-free.
- `checked_*` arithmetic is used for length calculations throughout the
  record / block parsers (e.g. `comp_len.checked_sub(2)` for block payload
  size).

### Decompression bombs

[`fastlz::decompress`](../src/fastlz/mod.rs) caps output size at
[`MAX_BLOCK_DECOMPRESSED`](../src/fastlz/mod.rs) (128 KiB). The check is
applied per literal byte pushed, so a malicious block cannot allocate
unbounded memory regardless of the input.

### Path traversal

Dirent names are sanitised through
[`safety::sanitize_8_3`](../src/safety.rs):

- Bytes outside printable ASCII become `_`.
- `/`, `\`, `:`, `*` are replaced with `_`.
- Leading `.` is collapsed to `_` (prevents hidden files and dotfile escapes).
- Names that contain no alphanumeric characters after sanitisation are
  rejected (caller should use [`safety::fallback_name`](../src/safety.rs)).

The output writer is responsible for:

- Verifying the resulting path does not contain `..` components
  ([`safety::contains_parent_traversal`](../src/safety.rs)).
- Resolving symlinks in the output directory before writing.

### Resource limits

- Block payload size is bounded at
  [`MAX_BLOCK_STORED`](../src/fastlz/mod.rs) (32 KiB + overhead).
- File offset tracking uses `u64` arithmetic with explicit overflow checks
  where offsets are added.
- Dirent count is bounded by the file size (no unbounded in-memory structure).
- The CLI accepts a `--max-output` flag (v0.1) to cap total extracted bytes.

### Format detection

The file header magic (`FEEF`) is checked before any further parsing. Bad
magic produces `Error::Format` and exits immediately — no allocation, no
seek past the header.

## Security tests

[`tests/security.rs`](../tests/security.rs) exercises every category above:

| Category | Test |
|---|---|
| Decompression bombs | `fastlz_uncompressed_block_rejects_oversized_length`, `fastlz_compressed_block_bounded_by_output_cap`, `fastlz_truncated_input_returns_error_not_panic` |
| Integer overflow | `ghost11_header_rejects_oversized_compression_byte`, `ghost11_header_rejects_compression_out_of_range` |
| Resource exhaustion | `ghostold_dirent_parses_with_adversarial_bytes` |
| Path traversal | `path_traversal_in_dirent_names_is_neutralised`, `path_traversal_with_absolute_prefix_is_neutralised`, `path_traversal_with_null_byte_is_neutralised` |
| No-panic-on-bad-input | `random_512_byte_buffers_parse_without_panic`, `random_dirent_buffer_parses_without_panic`, `fastlz_random_buffers_do_not_panic`, `ghost11_extract_on_random_bytes_does_not_panic`, `ghostold_walk_on_random_bytes_does_not_panic` |
| Format detection confusion | `span_header_detection_rejects_garbage`, `span_header_detection_accepts_realistic_headers` |

Plus [`fuzz/`](../fuzz/) provides libFuzzer harnesses for continuous
property-based fuzzing of `fastlz_decompress`, `ghost11_parse`, and
`ghostold_parse` targets (run nightly in CI; locally with
`cargo +nightly fuzz run <target>`).

## What this document does NOT cover

- **Encrypted images**: rejected at the file-header level
  (`Error::Encrypted`). No key recovery, no plaintext leak.
- **Malicious output directories**: if `--out` is itself a symlink to a
  sensitive location, `gho` will write into it. Users are responsible for
  verifying the output path.
- **TOCTOU on the input file**: `gho` opens the image, reads it, and exits.
  If an attacker can replace the file between the read and the write, the
  results are undefined. This is generally fine for forensic / extraction
  workflows where the input is on read-only media, but is documented here
  for completeness.

## Reporting security issues

Please open a private security advisory on GitHub:
<https://github.com/tomeq82/gho/security/advisories/new>.
