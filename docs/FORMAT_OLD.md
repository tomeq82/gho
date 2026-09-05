# Pre-11.x image format

This document specifies the binary format used by Norton Ghost versions
**before 11.x** (verified on a 2001-era Ghost 11.5.1 image of a Windows 95
ThinkPad). The format is fundamentally different from 11.x: instead of
partition records, the image contains a flat DFS directory of FAT-style
56-byte dirents.

## File header (512 bytes)

Same wire format as 11.x (`FEEF` magic + `file_type` + `compression` +
`image_id`). In observed images:

- `file_type == 1` (first file)
- `compression == 2` (FastLZ — the only compression observed)
- `image_id == 0x3bf50224` for `laptopas.gho`, +1 for each continuation span

## Record header (10 bytes)

Identical wire format to 11.x — same `RECORD_MAGIC = 0x012F18D8`,
same 2-byte type + 2-byte padding + 4-byte magic + 2-byte body_len.
Type codes are different:

| Code | Name | Description |
|---|---|---|
| `0x2C17` | BOOT_HMR | HMR / read-record at the very start of the image. Body: 6 bytes. |
| `0x2C04` | FIRST_DIRENT | First dirent in the image (singleton). Body: 56-byte dirent. |
| `0x0104` | DIRENT | A normal directory entry. Body: 56-byte dirent. |
| `0x0002` | DATA_FULL | Full 32 KiB compressed data block. |
| `0x0102` | DATA_LAST | Last (partial) compressed data block. |
| `0x0103` | DATA_TRAILER | 20-byte trailer after the last data block. |
| `0x0118` | PART2_BOOT | Boot sector of second partition (FAT32). |
| `0x0117` | PART2_TABLE | 512-byte partition table of second partition. |

## Dirent (56 bytes, FAT-style)

```
offset  size  field
------  ----  -----
0       8     name (space-padded 8.3)
8       3     extension (space-padded)
11      1     attributes (FAT bitmask)
14      2     ctime (FAT time)
16      2     cdate (FAT date)
18      2     adate (FAT date, last access)
20      2     cluster_hi
22      2     mtime
24      2     mdate
26      2     cluster_lo
28      4     size (LE u32, bytes)
32..    ...   padding to 56
```

FAT attributes:
- `0x10` = directory
- `0x20` = archive (regular file)
- `0x0F` = VFAT long-name fragment (all four low bits set)

## File pattern

Per the original `history-recovery` research notes:

```
[dirent 0x0104] [0x0002 ×N] [0x0102 ×1] [0x0103]
```

i.e. each file is: one dirent record, then N full 32 KiB FastLZ blocks,
then one partial last block, then a 20-byte trailer. Empty dirents
(directories, zero-byte files) have only the dirent record with no data
records.

`N = size / 32 KiB`, and the last block decompresses to `size % 32 KiB`
bytes. The walker's `WalkedEntry` precomputes both values.

## FastLZ

Identical wire format to 11.x. Same `src/fastlz/mod.rs::decompress`.

## Spanned images — the tricky bit

In pre-11.x images, **span boundaries can land inside a compressed data
block**, not at record boundaries. The Python survey script notes:

> granica spanu w tym formacie nie pokrywa się z granicą rekordu.
> Osadzony 512-bajtowy nagłówek pliku pojawia się w środku trwającego
> rekordu danych (np. w środku 32 KB skompresowanego bloku FastLZ).
> Poprawna obsługa: potraktować cały skonkatenowany strumień jako
> "logiczny" ciąg bajtów, z którego wycina się dokładnie te 512 bajtów w
> każdym znanym miejscu granicy.

The pre-stripped offsets are the cumulative file sizes. For the example
image:

- `laptopas.gho`: bytes 0..681_565_990 (start of span 1)
- `lapto001.GHS`: bytes 681_565_990..1_363_136_882 (start of span 2)
- `lapto002.GHS`: bytes 1_363_136_882..end (start of span 3)

`gho::span::concatenate_spans` automates this: it keeps the first file's
header and strips continuation headers (512 bytes at each known offset).

## Zero-padding gotcha

Real images often have a gap of zero-padding between the file header and
the first record. The walker scans forward up to 200 KB looking for
`RECORD_MAGIC` (`0x012F18D8`) at offset +4 of any candidate position. This
mirrors the tolerance of the Python survey scripts.

After the C: drive contents, the image may contain the FAT32 utility
partition (starting with record `0x0118`). The walker stops gracefully
when it can no longer find valid records.

## Sources

- `history-recovery/docs/handover/GHOST_OLD_FORMAT.md` — original reverse
  engineering notes (2026-09-04)
- `history-recovery/scripts/ghost-old-format-2001-survey-full.py` —
  Python survey implementation that the walker was ported from
- `history-recovery/scripts/ghost-old-format-2001-walk.py`,
  `ghost-old-format-2001-extract-{gadu-gadu,thebat,documents}.py` —
  one-shot extractors for specific files in the audit corpus
