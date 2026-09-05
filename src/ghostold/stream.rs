//! Streaming walker and extractor for pre-11.x Ghost images.
//!
//! See `docs/FORMAT_OLD.md` for the full record layout. In short: the image
//! is a flat DFS stream of dirents separated by their compressed data
//! blocks (when present). Reconstructing the full hierarchical directory
//! tree is intentionally not supported (see `docs/KNOWN_LIMITATIONS.md`).
//!
//! Two main entry points:
//! - [`walk_dirents`]: read the whole directory, returning one [`WalkedEntry`]
//!   per dirent with the file's data block positions pre-computed.
//! - [`extract_file`]: extract a single file by its [`WalkedEntry`].

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::{Error, Result};
use crate::fastlz;
use crate::ghostold::dirent::{DIRENT_SIZE, Dirent};
use crate::ghostold::record::{RECORD_HEADER_SIZE, Record, RecordType};
use crate::ghostold::{DATA_FULL_BLOCK_SIZE, HEADER_SIZE};

/// A simple wrapper around `File` that maintains a small lookahead buffer so
/// we can detect record headers vs embedded file headers without losing
/// track of the underlying file position.
struct LookaheadReader {
    file: File,
    buf: Vec<u8>,
    /// Number of valid bytes in `buf`.
    buf_len: usize,
    /// Cursor within `buf` (the next byte to consume is at `buf[buf_pos]`).
    buf_pos: usize,
}

impl LookaheadReader {
    fn new(file: File) -> Self {
        Self {
            file,
            buf: vec![0u8; 8192],
            buf_len: 0,
            buf_pos: 0,
        }
    }

    /// Ensure that `n` bytes are available starting at the current logical
    /// position. Returns `false` at EOF.
    fn ensure(&mut self, n: usize) -> Result<bool> {
        while self.buf_len - self.buf_pos < n {
            let mut tmp = [0u8; 8192];
            let r = self.file.read(&mut tmp)?;
            if r == 0 {
                if self.buf_len - self.buf_pos < n {
                    return Ok(false);
                }
                return Ok(true);
            }
            // Append to buf, growing if necessary.
            let need = self.buf_len + r;
            if self.buf.capacity() < need {
                self.buf.resize(need.max(self.buf.capacity() * 2), 0);
            }
            self.buf[self.buf_len..self.buf_len + r].copy_from_slice(&tmp[..r]);
            self.buf_len += r;
        }
        Ok(true)
    }

    fn peek(&mut self, n: usize) -> Result<Option<&[u8]>> {
        if !self.ensure(n)? {
            return Ok(None);
        }
        Ok(Some(&self.buf[self.buf_pos..self.buf_pos + n]))
    }

    fn consume(&mut self, n: usize) {
        debug_assert!(n <= self.buf_len - self.buf_pos, "consume past end");
        self.buf_pos += n;
        // Compact occasionally to avoid unbounded growth. We keep the
        // underlying allocation but reset positions so future reads overwrite
        // the buffer from the start.
        if self.buf_pos > 65_536 && self.buf_pos == self.buf_len {
            self.buf_pos = 0;
            self.buf_len = 0;
        }
    }

    /// Read exactly `n` bytes into `out` (consuming them).
    fn read_exact(&mut self, out: &mut [u8]) -> Result<()> {
        let n = out.len();
        if !self.ensure(n)? {
            return Err(Error::truncated(0, n, self.buf_len - self.buf_pos));
        }
        out.copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + n]);
        self.consume(n);
        Ok(())
    }

    fn skip(&mut self, n: u64) -> Result<()> {
        let mut remaining = n;
        let mut sink = [0u8; 8192];
        while remaining > 0 {
            let to_read = remaining.min(sink.len() as u64) as usize;
            self.read_exact(&mut sink[..to_read])?;
            remaining -= to_read as u64;
        }
        Ok(())
    }
}

/// One entry returned by [`walk_dirents`].
#[derive(Debug, Clone)]
pub struct WalkedEntry {
    /// Offset (in the logical stream) of the dirent record header.
    pub dirent_offset: u64,
    /// The parsed dirent.
    pub dirent: Dirent,
    /// Offset of the first data record (or `None` if the dirent has size 0
    /// and no data records follow).
    pub data_start_offset: Option<u64>,
    /// Number of full `0x0002` blocks the file occupies, derived from
    /// `dirent.size`. Zero for empty files.
    pub full_block_count: usize,
    /// Length in bytes of the trailing `0x0102` block after decompression,
    /// or 0 if the file is an exact multiple of 32 KiB.
    pub last_block_decompressed_size: usize,
    /// True if the dirent is an empty placeholder (size == 0 and no data).
    pub is_empty: bool,
}

/// Read the directory of a pre-11.x image.
///
/// `image_path` should already point to a single logical stream — for spanned
/// images, concatenate the physical files first (see
/// [`crate::span::concatenate_spans`]). The walker skips embedded 512-byte
/// file headers whenever it encounters one.
pub fn walk_dirents(image_path: &Path) -> Result<Vec<WalkedEntry>> {
    let file = File::open(image_path)?;
    let mut reader = LookaheadReader::new(file);
    let mut entries = Vec::new();

    // Skip the file header.
    let mut hdr = [0u8; HEADER_SIZE];
    reader.read_exact(&mut hdr)?;

    let mut offset: u64 = HEADER_SIZE as u64;
    let mut first_dirent_seen = false;

    // After the file header, real records may not start immediately — some
    // pre-11.x images have a gap of zero padding (or other inert bytes)
    // before the first valid record. Scan forward up to a reasonable limit
    // looking for the RECORD_MAGIC at offset +4 of any candidate position.
    // This mirrors the tolerance of the Python `ghost-old-format-2001-*`
    // scripts.
    const RECORD_SCAN_LIMIT: usize = 200_000;
    scan_for_record_magic(&mut reader, &mut offset, RECORD_SCAN_LIMIT)?;

    while let Some(peeked) = reader.peek(RECORD_HEADER_SIZE)? {
        // Stop gracefully if the next bytes don't look like a record or an
        // embedded file header — this matches the Python survey scripts,
        // which break on bad magic. The pre-11.x format puts the C: drive
        // contents first; the FAT32 utility partition that follows uses a
        // different layout that we don't currently parse (see KNOWN_LIMITATIONS).
        if peeked.len() >= 8
            && u32::from_le_bytes([peeked[4], peeked[5], peeked[6], peeked[7]])
                != crate::ghostold::record::RECORD_MAGIC
        {
            break;
        }

        // Record header?
        if let Some(rec) = Record::parse_at(peeked, 0) {
            reader.consume(RECORD_HEADER_SIZE);
            let body_len = rec.body_len as u64;
            let body_start = offset + RECORD_HEADER_SIZE as u64;
            offset = body_start + body_len;

            match rec.kind {
                RecordType::BootHmr | RecordType::Part2Boot | RecordType::Part2Table => {
                    if body_len > 0 {
                        reader.skip(body_len)?;
                    }
                }
                RecordType::FirstDirent => {
                    if !first_dirent_seen {
                        let mut body = vec![0u8; body_len as usize];
                        reader.read_exact(&mut body)?;
                        if body.len() >= DIRENT_SIZE {
                            let d = Dirent::parse(&body[..DIRENT_SIZE])?;
                            let data_start = if d.size > 0 {
                                Some(body_start + DIRENT_SIZE as u64)
                            } else {
                                None
                            };
                            entries.push(make_entry(
                                body_start - RECORD_HEADER_SIZE as u64,
                                d,
                                data_start,
                            ));
                        }
                        first_dirent_seen = true;
                    } else {
                        reader.skip(body_len)?;
                    }
                }
                RecordType::Dirent => {
                    let mut body = vec![0u8; body_len as usize];
                    reader.read_exact(&mut body)?;
                    if body.len() >= DIRENT_SIZE {
                        let d = Dirent::parse(&body[..DIRENT_SIZE])?;
                        let data_start = if d.size > 0 {
                            Some(body_start + DIRENT_SIZE as u64)
                        } else {
                            None
                        };
                        entries.push(make_entry(
                            body_start - RECORD_HEADER_SIZE as u64,
                            d,
                            data_start,
                        ));
                    }
                }
                RecordType::DataFull | RecordType::DataLast | RecordType::DataTrailer => {
                    // Data records belong to the previously emitted dirent;
                    // skip them silently. The walker only yields dirents, not
                    // the data itself — see `extract_file` for retrieval.
                    if body_len > 0 {
                        reader.skip(body_len)?;
                    }
                }
                RecordType::Unknown(_) => {
                    if body_len > 0 {
                        reader.skip(body_len)?;
                    }
                }
            }
            continue;
        }

        // Embedded 512-byte file header? Detect via the magic + file_type
        // in bytes 0..3 — we don't need the full 512 bytes here.
        let (looks_embedded, peek_len) = {
            let looks = peeked.len() >= 3
                && u16::from_le_bytes([peeked[0], peeked[1]]) == crate::ghost11::GHO_MAGIC
                && (peeked[2] == 1 || peeked[2] == 9);
            (looks, peeked.len())
        };
        if looks_embedded {
            if !reader.ensure(HEADER_SIZE)? {
                return Err(Error::truncated(offset, HEADER_SIZE, peek_len));
            }
            reader.consume(HEADER_SIZE);
            offset += HEADER_SIZE as u64;
            continue;
        }

        // Anything else is a format error.
        return Err(Error::format(
            offset,
            "expected record header or embedded file header, found non-matching bytes",
        ));
    }

    Ok(entries)
}

fn make_entry(dirent_offset: u64, dirent: Dirent, data_start_offset: Option<u64>) -> WalkedEntry {
    let size = dirent.size as usize;
    let full_block_count = size / DATA_FULL_BLOCK_SIZE;
    let last_block_decompressed_size = size % DATA_FULL_BLOCK_SIZE;
    let is_empty = size == 0 && data_start_offset.is_none();
    WalkedEntry {
        dirent_offset,
        dirent,
        data_start_offset,
        full_block_count,
        last_block_decompressed_size,
        is_empty,
    }
}

/// Advance `reader` past any padding bytes until a position whose bytes 4..8
/// are the [`RECORD_MAGIC`]. Updates `offset` to reflect the new logical
/// position. The scan is bounded by `limit` bytes.
fn scan_for_record_magic(
    reader: &mut LookaheadReader,
    offset: &mut u64,
    limit: usize,
) -> Result<()> {
    let mut scanned: usize = 0;
    while scanned < limit {
        let peeked = match reader.peek(RECORD_HEADER_SIZE)? {
            Some(b) => b,
            None => return Ok(()), // EOF
        };
        if peeked.len() >= 8
            && u32::from_le_bytes([peeked[4], peeked[5], peeked[6], peeked[7]])
                == crate::ghostold::record::RECORD_MAGIC
        {
            return Ok(());
        }
        reader.consume(1);
        *offset += 1;
        scanned += 1;
    }
    Ok(())
}

/// Extract a single file from a pre-11.x image to `out_path`.
///
/// `entry` must come from [`walk_dirents`] on the same image. The function
/// re-opens the image and seeks to `entry.data_start_offset` to read the
/// compressed data blocks.
pub fn extract_file(image_path: &Path, entry: &WalkedEntry, out_path: &Path) -> Result<u64> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if entry.is_empty || entry.data_start_offset.is_none() {
        std::fs::write(out_path, b"")?;
        return Ok(0);
    }
    let file = File::open(image_path)?;
    let mut reader = LookaheadReader::new(file);
    reader
        .file
        .seek(SeekFrom::Start(entry.data_start_offset.unwrap()))?;

    use std::io::BufWriter;
    use std::io::Write;
    let out_file = File::create(out_path)?;
    let mut writer = BufWriter::new(out_file);

    let mut written: u64 = 0;
    let expected_total = entry.dirent.size as u64;

    for _ in 0..entry.full_block_count {
        let mut rec_hdr = [0u8; RECORD_HEADER_SIZE];
        reader.read_exact(&mut rec_hdr)?;
        let rec = Record::parse_at(&rec_hdr, 0)
            .ok_or_else(|| Error::format(0, "expected DATA_FULL record header"))?;
        if rec.kind != RecordType::DataFull {
            return Err(Error::format(
                0,
                format!(
                    "expected DATA_FULL, got {:?} body_len={}",
                    rec.kind, rec.body_len
                ),
            ));
        }
        if rec.body_len as usize != DATA_FULL_BLOCK_SIZE {
            return Err(Error::format(
                0,
                format!(
                    "DATA_FULL body_len {} != {}",
                    rec.body_len, DATA_FULL_BLOCK_SIZE
                ),
            ));
        }
        let mut body = vec![0u8; rec.body_len as usize];
        reader.read_exact(&mut body)?;
        let decompressed = fastlz::decompress(&body, body.len())?;
        if decompressed.len() != DATA_FULL_BLOCK_SIZE {
            return Err(Error::format(
                0,
                format!(
                    "DATA_FULL decompressed {} bytes != {}",
                    decompressed.len(),
                    DATA_FULL_BLOCK_SIZE
                ),
            ));
        }
        writer.write_all(&decompressed)?;
        written += decompressed.len() as u64;
    }

    if entry.last_block_decompressed_size > 0 {
        let mut rec_hdr = [0u8; RECORD_HEADER_SIZE];
        reader.read_exact(&mut rec_hdr)?;
        let rec = Record::parse_at(&rec_hdr, 0)
            .ok_or_else(|| Error::format(0, "expected DATA_LAST record header"))?;
        if rec.kind != RecordType::DataLast {
            return Err(Error::format(
                0,
                format!("expected DATA_LAST, got {:?}", rec.kind),
            ));
        }
        let mut body = vec![0u8; rec.body_len as usize];
        reader.read_exact(&mut body)?;
        let decompressed = fastlz::decompress(&body, body.len())?;
        if decompressed.len() != entry.last_block_decompressed_size {
            return Err(Error::format(
                0,
                format!(
                    "DATA_LAST decompressed {} bytes != {}",
                    decompressed.len(),
                    entry.last_block_decompressed_size
                ),
            ));
        }
        writer.write_all(&decompressed)?;
        written += decompressed.len() as u64;
    }

    let mut trailer_hdr = [0u8; RECORD_HEADER_SIZE];
    reader.read_exact(&mut trailer_hdr)?;
    let rec = Record::parse_at(&trailer_hdr, 0)
        .ok_or_else(|| Error::format(0, "expected DATA_TRAILER record header"))?;
    if rec.kind != RecordType::DataTrailer {
        return Err(Error::format(
            0,
            format!("expected DATA_TRAILER, got {:?}", rec.kind),
        ));
    }
    let mut trailer_body = vec![0u8; rec.body_len as usize];
    reader.read_exact(&mut trailer_body)?;

    writer.flush()?;
    drop(writer);

    if written != expected_total {
        return Err(Error::format(
            entry.dirent_offset,
            format!(
                "decompressed {} bytes != dirent.size {}",
                written, expected_total
            ),
        ));
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghostold::record::{
        RECORD_MAGIC, RECORD_TYPE_DATA_FULL, RECORD_TYPE_DATA_LAST, RECORD_TYPE_DATA_TRAILER,
        RECORD_TYPE_DIRENT, RECORD_TYPE_FIRST_DIRENT,
    };

    fn record(type_code: u16, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&type_code.to_le_bytes());
        out.extend_from_slice(&[0u8; 2]);
        out.extend_from_slice(&RECORD_MAGIC.to_le_bytes());
        out.extend_from_slice(&(body.len() as u16).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn file_header() -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[2] = 1;
        buf[3] = 2;
        buf
    }

    fn dirent(name: &str, ext: &str, size: u32) -> Vec<u8> {
        let mut d = [0u8; DIRENT_SIZE];
        let n = name.as_bytes();
        let e = ext.as_bytes();
        d[..n.len()].copy_from_slice(n);
        d[8..8 + e.len()].copy_from_slice(e);
        if size > 0 {
            d[11] = 0x20;
        } else if !name.is_empty() {
            d[11] = 0x10;
        }
        d[28..32].copy_from_slice(&size.to_le_bytes());
        d.to_vec()
    }

    fn build_stream_one_file(name: &str, ext: &str, payload: &[u8]) -> Vec<u8> {
        let mut s = file_header();
        s.extend(
            record(
                RECORD_TYPE_FIRST_DIRENT,
                &dirent(name, ext, payload.len() as u32),
            )
            .iter(),
        );
        let full = payload.len() / DATA_FULL_BLOCK_SIZE;
        let last = payload.len() % DATA_FULL_BLOCK_SIZE;
        for i in 0..full {
            let chunk = &payload[i * DATA_FULL_BLOCK_SIZE..(i + 1) * DATA_FULL_BLOCK_SIZE];
            let mut blk = vec![1u8, 0, 0, 0];
            let n = chunk.len() as u32;
            blk[1] = (n & 0xFF) as u8;
            blk[2] = ((n >> 8) & 0xFF) as u8;
            blk[3] = ((n >> 16) & 0xFF) as u8;
            blk.extend_from_slice(chunk);
            s.extend(record(RECORD_TYPE_DATA_FULL, &blk).iter());
        }
        if last > 0 {
            let chunk = &payload[full * DATA_FULL_BLOCK_SIZE..];
            let mut blk = vec![1u8, 0, 0, 0];
            let n = chunk.len() as u32;
            blk[1] = (n & 0xFF) as u8;
            blk[2] = ((n >> 8) & 0xFF) as u8;
            blk[3] = ((n >> 16) & 0xFF) as u8;
            blk.extend_from_slice(chunk);
            s.extend(record(RECORD_TYPE_DATA_LAST, &blk).iter());
        }
        s.extend(record(RECORD_TYPE_DATA_TRAILER, &[0u8; 20]).iter());
        s
    }

    #[test]
    fn walk_one_file() {
        let payload = b"hello pre-11x";
        let stream = build_stream_one_file("HELLO   ", "TXT", payload);
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].dirent.display_name(), "HELLO.TXT");
        assert_eq!(entries[0].dirent.size, payload.len() as u32);
    }

    #[test]
    fn walk_empty_dirents() {
        let stream = file_header();
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn extract_one_file_via_uncompressed_escape() {
        let payload = b"round-trip-payload".repeat(20);
        let stream = build_stream_one_file("ROUND   ", "TXT", &payload);
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let entries = walk_dirents(&img).unwrap();
        let out = tmp.path().join("out").join("ROUND.TXT");
        let written = extract_file(&img, &entries[0], &out).unwrap();
        assert_eq!(written, payload.len() as u64);
        let got = std::fs::read(&out).unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn walk_skips_embedded_file_header() {
        let mut stream = file_header();
        stream.extend(file_header().iter());
        stream.extend(record(RECORD_TYPE_FIRST_DIRENT, &dirent("A       ", "TXT", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].dirent.display_name(), "A.TXT");
    }

    #[test]
    fn walk_with_multiple_dirents() {
        let mut s = file_header();
        s.extend(record(RECORD_TYPE_FIRST_DIRENT, &dirent("A       ", "TXT", 0)).iter());
        s.extend(record(RECORD_TYPE_DIRENT, &dirent("B       ", "EXE", 0)).iter());
        s.extend(record(RECORD_TYPE_DIRENT, &dirent("C       ", "DAT", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &s).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].dirent.display_name(), "A.TXT");
        assert_eq!(entries[1].dirent.display_name(), "B.EXE");
        assert_eq!(entries[2].dirent.display_name(), "C.DAT");
    }

    #[test]
    fn data_records_between_dirents_are_silently_skipped() {
        // Format: dirent A (empty) -> data_full block -> dirent B (empty)
        // The walker should yield both dirents and silently skip the data record.
        let mut s = file_header();
        s.extend(record(RECORD_TYPE_FIRST_DIRENT, &dirent("A       ", "TXT", 0)).iter());
        s.extend(record(RECORD_TYPE_DATA_FULL, &[0u8; 32]).iter());
        s.extend(record(RECORD_TYPE_DIRENT, &dirent("B       ", "TXT", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &s).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].dirent.display_name(), "A.TXT");
        assert_eq!(entries[1].dirent.display_name(), "B.TXT");
    }

    #[test]
    fn unknown_record_type_is_skipped() {
        let mut s = file_header();
        s.extend(record(0xABCD, &[0u8; 10]).iter());
        s.extend(record(RECORD_TYPE_FIRST_DIRENT, &dirent("A       ", "TXT", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &s).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn extract_empty_dirent_writes_empty_file() {
        let mut s = file_header();
        s.extend(record(RECORD_TYPE_FIRST_DIRENT, &dirent("EMPTY   ", "   ", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &s).unwrap();
        let entries = walk_dirents(&img).unwrap();
        let out = tmp.path().join("EMPTY");
        let written = extract_file(&img, &entries[0], &out).unwrap();
        assert_eq!(written, 0);
        assert_eq!(std::fs::read(&out).unwrap().len(), 0);
    }

    #[test]
    fn walk_handles_data_records_between_dirents() {
        // Format: dirent A (with data) -> dirent B (empty)
        let payload = b"compressed-data".repeat(50);
        let mut s = file_header();
        // First dirent with data
        s.extend(
            record(
                RECORD_TYPE_FIRST_DIRENT,
                &dirent("DATA    ", "BIN", payload.len() as u32),
            )
            .iter(),
        );
        // Data full + last + trailer
        let full = payload.len() / DATA_FULL_BLOCK_SIZE;
        let last = payload.len() % DATA_FULL_BLOCK_SIZE;
        for i in 0..full {
            let chunk = &payload[i * DATA_FULL_BLOCK_SIZE..(i + 1) * DATA_FULL_BLOCK_SIZE];
            let mut blk = vec![1u8, 0, 0, 0];
            let n = chunk.len() as u32;
            blk[1] = (n & 0xFF) as u8;
            blk[2] = ((n >> 8) & 0xFF) as u8;
            blk[3] = ((n >> 16) & 0xFF) as u8;
            blk.extend_from_slice(chunk);
            s.extend(record(RECORD_TYPE_DATA_FULL, &blk).iter());
        }
        if last > 0 {
            let chunk = &payload[full * DATA_FULL_BLOCK_SIZE..];
            let mut blk = vec![1u8, 0, 0, 0];
            let n = chunk.len() as u32;
            blk[1] = (n & 0xFF) as u8;
            blk[2] = ((n >> 8) & 0xFF) as u8;
            blk[3] = ((n >> 16) & 0xFF) as u8;
            blk.extend_from_slice(chunk);
            s.extend(record(RECORD_TYPE_DATA_LAST, &blk).iter());
        }
        s.extend(record(RECORD_TYPE_DATA_TRAILER, &[0u8; 20]).iter());
        // Second dirent, no data
        s.extend(record(RECORD_TYPE_DIRENT, &dirent("NEXT    ", "TXT", 0)).iter());
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &s).unwrap();
        let entries = walk_dirents(&img).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].dirent.display_name(), "DATA.BIN");
        assert_eq!(entries[1].dirent.display_name(), "NEXT.TXT");
        // Extract the first file
        let out = tmp.path().join("DATA.BIN");
        let written = extract_file(&img, &entries[0], &out).unwrap();
        assert_eq!(written, payload.len() as u64);
        assert_eq!(std::fs::read(&out).unwrap(), payload);
    }

    #[test]
    fn lookahead_reader_basic() {
        use std::io::Write;
        let f = tempfile::tempdir().unwrap();
        let path = f.path().join("x");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"abcdefghij").unwrap();
        f.flush().ok();
        drop(f);
        let f = File::open(&path).unwrap();
        let mut lr = LookaheadReader::new(f);
        assert_eq!(lr.peek(5).unwrap().unwrap(), b"abcde");
        assert_eq!(lr.peek(10).unwrap().unwrap(), b"abcdefghij");
        lr.consume(3);
        assert_eq!(lr.peek(5).unwrap().unwrap(), b"defgh");
        let mut out = [0u8; 3];
        lr.read_exact(&mut out).unwrap();
        assert_eq!(&out, b"def");
    }
}
