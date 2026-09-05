# Ghost 11.x / 12.x image format

This document specifies the binary format used by Norton Ghost 11.x and
12.x (and later), as reverse-engineered from the file headers and record
streams produced by `gho create` and Norton Ghost 11.5.1.

## File header (512 bytes)

Every `.gho` and `.ghs` file starts with a 512-byte header:

```
offset  size  field
------  ----  -----
0       2     magic (LE u16): 0xEFFE ("FEEF" backwards — common Ghost tell)
2       1     file_type: 1 = first/single file, 9 = span continuation
3       1     compression: 0 = none, 2 = FastLZ, 3..=9 = zlib
4       4     image_id (LE u32): shared across all span files of one image
8       4     flags / unknown
12      1     encryption flag (bit 1 set = encrypted)
13..    ...   padding
```

The parser refuses images with `magic != 0xEFFE` or
`file_type ∉ {1, 9}`.

## Record layer (10-byte header + body)

A Ghost 11.x image is a stream of records. Each record has a 10-byte
header followed by a variable-length body:

```
offset  size  field
------  ----  -----
0       2     type_code (LE u16)
2       2     padding (typically zero)
4       4     magic (LE u32): 0x012F18D8
8       2     body_len (LE u16)
10..    ...   body (body_len bytes)
```

### Known record type codes

| Code | Name | Description |
|---|---|---|
| `0x0006` | TRACK0 | First record. Body: 6-byte mini header + optional 512-byte MBR. |
| `0x0603` | PARTITION | Marks the start of a new partition payload. |
| `0x0703` | CONTINUATION | Links a spanned image to the next physical file. |
| `0x0023` | END | Terminates the image stream. |

## Block layer (compressed payload)

Between records, the image contains a stream of compressed blocks. Each
block is framed by:

```
offset  size  field
------  ----  -----
0       2     stored_len (LE u16): payload length + 2
2..     ...   payload (stored_len - 2 bytes)
```

The payload is decompressed according to the file header's `compression`
field:

| Compression | Decompressor |
|---|---|
| 0 (none) | payload is stored verbatim |
| 2 (FastLZ) | Ghost's "Fast LZ (Z1)" — see `src/fastlz/mod.rs` |
| 3..=9 (zlib) | standard zlib (RFC 1950) |
| other | rejected with `Error::UnsupportedCompression` |

Each partition has `size / 32 KiB` full blocks followed by one optional
partial block.

## Spanned (multi-volume) images

A single logical image can span multiple physical files (`.gho`, `.ghs`,
`.ghs`, ...). Each physical file starts with its own 512-byte file header.
After concatenation, the parser must:

- Keep the first file's header (parsers expect it at offset 0).
- Strip continuation headers (file_type = 9) at the file boundaries —
  they appear in the middle of the logical stream and would otherwise be
  misinterpreted as partition records.

`gho::span::concatenate_spans` does this automatically.

## Encryption

The encryption bit in byte 12 of the file header is reserved but not
implemented. Encrypted images raise `Error::Encrypted`. See
[`KNOWN_LIMITATIONS.md`](KNOWN_LIMITATIONS.md).

## Sources

- Reverse engineering notes from `history-recovery` (Norton Ghost 11.5.1
  record layout).
- `python3 -c "import sys; sys.path.insert(0, '.'); from history_recovery
  import ghost_image; help(ghost_image.extract)"`
