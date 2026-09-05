//! Streaming extractor for Ghost 11.x / 12.x images.
//!
//! Walks the record stream, decompressing every compressed block into its
//! corresponding partition file. Writes are streamed — the entire image is
//! never held in memory, so multi-hundred-gigabyte images work.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::fastlz;
use crate::ghost11::{
    Compression, HEADER_SIZE,
    header::FileHeader,
    looks_like_embedded_file_header, looks_like_record,
    record::{RECORD_HEADER_SIZE, RecordType},
};
use crate::mbr::parse as parse_mbr;

/// Maximum length of a single compressed / uncompressed block payload
/// (excluding the 2-byte stored_len prefix that wraps every block on disk).
const MAX_BLOCK_STORED: usize = 32 * 1024 + 4 + 2;

/// Result of extracting a single Ghost 11.x / 12.x image.
#[derive(Debug)]
pub struct ExtractResult {
    pub header: FileHeader,
    pub mbr_entries: Vec<crate::mbr::MbrEntry>,
    pub partitions: Vec<PartitionSummary>,
}

/// Summary of one extracted partition.
#[derive(Debug, Clone)]
pub struct PartitionSummary {
    pub index: usize,
    pub mbr_type: Option<u8>,
    pub compressed_bytes: u64,
    pub decompressed_bytes: u64,
    pub output_path: PathBuf,
}

/// Read one block (2-byte stored_len prefix + payload) from the reader,
/// decompress it according to `compression`, and return the decompressed
/// bytes.
fn read_and_decompress_block<R: Read>(
    reader: &mut R,
    compression: Compression,
    offset: u64,
) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 2];
    reader.read_exact(&mut len_buf)?;
    let stored_len = u16::from_le_bytes(len_buf) as usize;
    if stored_len == 0 {
        return Ok(Vec::new());
    }
    let comp_len = stored_len
        .checked_sub(2)
        .ok_or_else(|| Error::format(offset, format!("invalid block stored_len={stored_len}")))?;
    if comp_len > MAX_BLOCK_STORED {
        return Err(Error::format(
            offset,
            format!("invalid block stored_len={stored_len} (max {MAX_BLOCK_STORED})"),
        ));
    }
    let mut block = vec![0u8; comp_len];
    reader.read_exact(&mut block)?;
    decompress_block(&block, compression, offset)
}

/// Decompress a block payload according to the image's compression type.
fn decompress_block(block: &[u8], compression: Compression, offset: u64) -> Result<Vec<u8>> {
    match compression {
        Compression::None => Ok(block.to_vec()),
        Compression::FastLz => fastlz::decompress(block, block.len()).map_err(|e| match e {
            Error::FastLz { message, .. } => Error::fastlz(offset, message),
            other => other,
        }),
        Compression::Zlib => {
            let mut out = Vec::with_capacity(block.len() * 4);
            let mut decoder = flate2::read::ZlibDecoder::new(block);
            let mut tmp = [0u8; 8192];
            loop {
                let n = decoder.read(&mut tmp)?;
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&tmp[..n]);
            }
            Ok(out)
        }
    }
}

/// Extract every partition from a Ghost 11.x / 12.x image.
///
/// `image_path` is the path to a single-file image. For spanned images,
/// concatenate the spans first (see [`crate::span::concatenate_spans`]).
///
/// Writes one raw partition image per partition to `out_dir`:
/// `partition_0.img`, `partition_1.img`, ...
pub fn extract(image_path: &Path, out_dir: &Path) -> Result<ExtractResult> {
    std::fs::create_dir_all(out_dir)?;
    let file = File::open(image_path)?;
    let mut reader = BufReader::new(file);

    let mut header_buf = [0u8; HEADER_SIZE];
    reader.read_exact(&mut header_buf)?;
    let header = FileHeader::parse(&header_buf)?;
    if header.encrypted {
        return Err(Error::Encrypted);
    }
    let compression = Compression::from_byte(header.compression)?;

    let mut mbr_entries: Vec<crate::mbr::MbrEntry> = Vec::new();
    let mut partitions: Vec<PartitionSummary> = Vec::new();

    let mut current_writer: Option<BufWriter<File>> = None;
    let mut current_path: Option<PathBuf> = None;
    let mut current_index: isize = -1;
    let mut current_compressed: u64 = 0;
    let mut current_decompressed: u64 = 0;

    let mut offset: u64 = HEADER_SIZE as u64;

    loop {
        // Try to parse a record header at the current offset.
        let mut peek = [0u8; RECORD_HEADER_SIZE];
        let n = match reader.read(&mut peek) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(Error::Io(e)),
        };
        if n == 0 {
            break;
        }
        // Rewind so the parser sees the whole header.
        reader.seek(SeekFrom::Current(-(n as i64)))?;

        if let Some(rec) = looks_like_record(&peek, 0) {
            // Consume the 10-byte header.
            let mut header = [0u8; RECORD_HEADER_SIZE];
            reader.read_exact(&mut header)?;
            offset += RECORD_HEADER_SIZE as u64;

            let body_start = offset;
            let body_len = rec.body_len as u64;
            let mut body = vec![0u8; body_len as usize];
            if body_len > 0 {
                reader.read_exact(&mut body)?;
                offset += body_len;
            }

            match rec.kind {
                RecordType::Track0 => {
                    // body starts with 6-byte mini header, then optionally a 512-byte MBR.
                    if body.len() >= 6 + 512 {
                        mbr_entries = parse_mbr(&body[6..6 + 512])?;
                    }
                }
                RecordType::Partition => {
                    // Finalise the previous partition if any.
                    if let Some(mut w) = current_writer.take() {
                        let _ = w.flush();
                    }
                    if let (Some(idx), Some(path)) =
                        (usize::try_from(current_index).ok(), current_path.take())
                    {
                        let mbr_type = if idx < mbr_entries.len() {
                            Some(mbr_entries[idx].part_type)
                        } else {
                            None
                        };
                        partitions.push(PartitionSummary {
                            index: idx,
                            mbr_type,
                            compressed_bytes: current_compressed,
                            decompressed_bytes: current_decompressed,
                            output_path: path,
                        });
                    }
                    // Body of a Partition record is typically 20 bytes of
                    // metadata; the next 512 bytes are an embedded file
                    // header that we skip.
                    if !looks_like_embedded_file_header(&body, 0) {
                        // Some implementations omit the embedded header; allow
                        // either way and just trust the next read.
                    } else {
                        // Embedded header was *inside* the body? No — in the
                        // Python code the body of a Partition record does NOT
                        // include the embedded header. The embedded header
                        // appears *after* the body. Re-check by peeking
                        // without consuming.
                    }

                    // Check whether an embedded file header follows the body.
                    let mut peek2 = [0u8; HEADER_SIZE];
                    let m = match reader.read(&mut peek2) {
                        Ok(m) => m,
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
                        Err(e) => return Err(Error::Io(e)),
                    };
                    let consumed_embedded = if looks_like_embedded_file_header(&peek2, 0) {
                        reader.seek(SeekFrom::Current(-(m as i64)))?;
                        // Consume the embedded header.
                        let mut embedded = [0u8; HEADER_SIZE];
                        reader.read_exact(&mut embedded)?;
                        offset += HEADER_SIZE as u64;
                        true
                    } else {
                        reader.seek(SeekFrom::Current(-(m as i64)))?;
                        false
                    };
                    let _ = consumed_embedded; // already consumed above

                    current_index += 1;
                    current_compressed = 0;
                    current_decompressed = 0;
                    current_path = Some(out_dir.join(format!("partition_{current_index}.img")));
                    let f = File::create(current_path.as_ref().unwrap())?;
                    current_writer = Some(BufWriter::new(f));
                }
                RecordType::Continuation => {
                    // Body length: skip body; if an embedded file header
                    // follows, skip that too.
                    if looks_like_embedded_file_header_at(&mut reader, offset)? {
                        offset += HEADER_SIZE as u64;
                    }
                }
                RecordType::End => {
                    if let Some(mut w) = current_writer.take() {
                        let _ = w.flush();
                    }
                    if let (Some(idx), Some(path)) =
                        (usize::try_from(current_index).ok(), current_path.take())
                    {
                        let mbr_type = if idx < mbr_entries.len() {
                            Some(mbr_entries[idx].part_type)
                        } else {
                            None
                        };
                        partitions.push(PartitionSummary {
                            index: idx,
                            mbr_type,
                            compressed_bytes: current_compressed,
                            decompressed_bytes: current_decompressed,
                            output_path: path,
                        });
                    }
                    break;
                }
            }
            let _ = body_start;
            continue;
        }

        if looks_like_embedded_file_header(&peek, 0) {
            // Skip the embedded 512-byte file header.
            let mut embedded = [0u8; HEADER_SIZE];
            reader.read_exact(&mut embedded)?;
            offset += HEADER_SIZE as u64;
            continue;
        }

        // Otherwise: a compressed block.
        if current_writer.is_none() {
            return Err(Error::format(
                offset,
                "compressed block outside any partition",
            ));
        }
        let block_offset = offset;
        let block = read_and_decompress_block(&mut reader, compression, block_offset)?;
        if let Some(w) = current_writer.as_mut() {
            w.write_all(&block)?;
        }
        current_compressed += 2 + block.len() as u64;
        current_decompressed += block.len() as u64;
        offset += 2 + block.len() as u64;
    }

    Ok(ExtractResult {
        header,
        mbr_entries,
        partitions,
    })
}

/// Look ahead `HEADER_SIZE` bytes without consuming; if they form an embedded
/// file header, leave the reader positioned just after them.
fn looks_like_embedded_file_header_at<R: Read + Seek>(
    reader: &mut R,
    _offset: u64,
) -> Result<bool> {
    let mut peek = [0u8; HEADER_SIZE];
    let n = match reader.read(&mut peek) {
        Ok(n) => n,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
        Err(e) => return Err(Error::Io(e)),
    };
    reader.seek(SeekFrom::Current(-(n as i64)))?;
    Ok(looks_like_embedded_file_header(&peek, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghost11::{
        HEADER_SIZE,
        record::{RECORD_TYPE_PARTITION, RECORD_TYPE_TRACK0, RecordType},
    };
    use std::io::Cursor;

    fn file_header(compression: u8) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[2] = 1;
        buf[3] = compression;
        buf
    }

    fn record(type_code: u16, body: &[u8]) -> Vec<u8> {
        use crate::ghost11::record::RECORD_MAGIC;
        let mut out = Vec::new();
        out.extend_from_slice(&type_code.to_le_bytes());
        out.extend_from_slice(&[0u8; 2]);
        out.extend_from_slice(&RECORD_MAGIC.to_le_bytes());
        out.extend_from_slice(&(body.len() as u16).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn block(payload: &[u8]) -> Vec<u8> {
        let stored_len = payload.len() as u16 + 2;
        let mut out = Vec::new();
        out.extend_from_slice(&stored_len.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn build_stream_uncompressed(payloads: &[Vec<u8>]) -> Vec<u8> {
        let mut out = file_header(0);
        out.extend(record(RECORD_TYPE_TRACK0, &[0u8; 6]).iter());
        for p in payloads {
            out.extend(record(RECORD_TYPE_PARTITION, &[0u8; 20]).iter());
            // embedded header follows body in real format
            out.extend(file_header(0).iter());
            out.extend(block(p).iter());
        }
        // End record
        use crate::ghost11::record::RECORD_TYPE_END;
        out.extend(record(RECORD_TYPE_END, &[0u8; 24]).iter());
        out
    }

    #[test]
    fn extract_uncompressed_single_partition() {
        let payload = b"personal-history-payload".repeat(10);
        let stream = build_stream_uncompressed(std::slice::from_ref(&payload));
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let out_dir = tmp.path().join("out");
        let result = extract(&img, &out_dir).unwrap();
        assert_eq!(result.partitions.len(), 1);
        let got = std::fs::read(&result.partitions[0].output_path).unwrap();
        assert_eq!(got, payload);
        assert_eq!(
            result.partitions[0].decompressed_bytes,
            payload.len() as u64
        );
    }

    #[test]
    fn extract_uncompressed_multiple_partitions() {
        let p1 = b"first-partition".repeat(5);
        let p2 = b"second-partition".repeat(7);
        let stream = build_stream_uncompressed(&[p1.clone(), p2.clone()]);
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let out_dir = tmp.path().join("out");
        let result = extract(&img, &out_dir).unwrap();
        assert_eq!(result.partitions.len(), 2);
        assert_eq!(
            std::fs::read(&result.partitions[0].output_path).unwrap(),
            p1
        );
        assert_eq!(
            std::fs::read(&result.partitions[1].output_path).unwrap(),
            p2
        );
    }

    #[test]
    fn extract_rejects_encrypted() {
        let mut stream = file_header(0);
        stream[12] = 0x02;
        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("enc.gho");
        std::fs::write(&img, &stream).unwrap();
        let out_dir = tmp.path().join("out");
        let err = extract(&img, &out_dir).unwrap_err();
        assert!(matches!(err, Error::Encrypted));
    }

    #[test]
    fn decompress_block_none() {
        let got = decompress_block(b"hello", Compression::None, 0).unwrap();
        assert_eq!(got, b"hello");
    }

    #[test]
    fn decompress_block_fastlz_uses_uncompressed_escape() {
        // byte[0] == 1 → stored uncompressed; bytes 1..4 = u24 length
        let mut payload = vec![1u8, 5, 0, 0];
        payload.extend_from_slice(b"hello");
        let got = decompress_block(&payload, Compression::FastLz, 0).unwrap();
        assert_eq!(got, b"hello");
    }

    #[test]
    fn decompress_block_zlib_roundtrip() {
        let raw = b"compressed payload repeated compressed payload repeated";
        let enc = {
            use flate2::write::ZlibEncoder;
            use std::io::Write;
            let mut z = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(raw).unwrap();
            z.finish().unwrap()
        };
        let got = decompress_block(&enc, Compression::Zlib, 0).unwrap();
        assert_eq!(got, raw);
    }

    #[test]
    fn block_payload_reads_two_byte_len() {
        // stored_len = 5 + 2 = 7; payload is "abcde"
        let mut buf = Vec::new();
        buf.extend_from_slice(&7u16.to_le_bytes());
        buf.extend_from_slice(b"abcde");
        let mut cursor = Cursor::new(buf);
        let got = read_and_decompress_block(&mut cursor, Compression::None, 0).unwrap();
        assert_eq!(got, b"abcde");
    }

    #[test]
    fn empty_block_returns_empty_vec() {
        let buf = vec![0u8, 0]; // stored_len = 0
        let mut cursor = Cursor::new(buf);
        let got = read_and_decompress_block(&mut cursor, Compression::None, 0).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn extract_with_mbr_partition_type() {
        // Build Track0 with MBR that has one NTFS entry
        let mut track0_body = vec![0u8; 6];
        let mut mbr = vec![0u8; 512];
        mbr[446 + 4] = 0x07; // NTFS
        mbr[446 + 8..446 + 12].copy_from_slice(&63u32.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&1000u32.to_le_bytes());
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        track0_body.extend_from_slice(&mbr);

        let mut stream = file_header(0);
        stream.extend(record(RECORD_TYPE_TRACK0, &track0_body).iter());
        stream.extend(record(RECORD_TYPE_PARTITION, &[0u8; 20]).iter());
        stream.extend(file_header(0).iter());
        stream.extend(block(b"payload").iter());
        use crate::ghost11::record::RECORD_TYPE_END;
        stream.extend(record(RECORD_TYPE_END, &[0u8; 24]).iter());

        let tmp = tempfile::tempdir().unwrap();
        let img = tmp.path().join("img.gho");
        std::fs::write(&img, &stream).unwrap();
        let out_dir = tmp.path().join("out");
        let result = extract(&img, &out_dir).unwrap();
        assert_eq!(result.mbr_entries.len(), 1);
        assert_eq!(result.mbr_entries[0].part_type, 0x07);
        assert_eq!(result.partitions[0].mbr_type, Some(0x07));
    }

    #[test]
    fn record_type_from_u16() {
        assert_eq!(RecordType::from_u16(0x0006), Some(RecordType::Track0));
        assert_eq!(RecordType::from_u16(0x0603), Some(RecordType::Partition));
        assert_eq!(RecordType::from_u16(0x0703), Some(RecordType::Continuation));
        assert_eq!(RecordType::from_u16(0x0023), Some(RecordType::End));
        assert_eq!(RecordType::from_u16(0x9999), None);
    }
}
