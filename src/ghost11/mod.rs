//! Norton Ghost 11.x / 12.x image parser.
//!
//! Format layout (see `docs/FORMAT.md`):
//! - 512-byte file header (`FEEF` magic).
//! - Stream of 10-byte record headers followed by `body_len` body bytes.
//! - Records types: Track0 (`0x0006`), Partition (`0x0603`), Continuation
//!   (`0x0703`), End (`0x0023`).
//! - Between records, embedded compressed blocks (2-byte `stored_len` + payload).
//!
//! Block payload is either stored uncompressed (`compression == 0`), FastLZ
//! (`compression == 2`), or zlib (`compression ∈ [3, 10)`).

use crate::error::{Error, Result};

/// `FEEF` magic at the start of every `.gho` / `.ghs` file.
pub const GHO_MAGIC: u16 = 0xEFFE;
/// Size of a single Ghost file header in bytes.
pub const HEADER_SIZE: usize = 512;
/// Size of a record header in bytes.
pub const RECORD_HEADER_SIZE: usize = 10;
/// Magic number embedded in every record header (little-endian).
pub const RECORD_MAGIC: u32 = 0x012F_18D8;

/// Track0 record — appears once at the start; body starts with a 6-byte mini
/// header and (optionally) a 512-byte MBR.
pub const RECORD_TYPE_TRACK0: u16 = 0x0006;
/// Partition record — marks the start of a new partition payload.
pub const RECORD_TYPE_PARTITION: u16 = 0x0603;
/// Continuation record — links a spanned image to the next physical file.
pub const RECORD_TYPE_CONTINUATION: u16 = 0x0703;
/// End record — terminates the image stream.
pub const RECORD_TYPE_END: u16 = 0x0023;

/// All known record types.
pub const KNOWN_RECORD_TYPES: &[u16] = &[
    RECORD_TYPE_TRACK0,
    RECORD_TYPE_PARTITION,
    RECORD_TYPE_CONTINUATION,
    RECORD_TYPE_END,
];

/// No compression (blocks are stored verbatim).
pub const COMPRESSION_NONE: u8 = 0;
/// FastLZ (Z1) compression.
pub const COMPRESSION_FAST: u8 = 2;

/// Parsed 512-byte file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    /// 1 = first/single file, 9 = span continuation file.
    pub file_type: u8,
    /// 0 = none, 2 = FastLZ, 3..10 = zlib.
    pub compression: u8,
    /// Shared identifier across all span files of one logical image.
    pub image_id: u32,
    /// True if the encryption flag bit is set.
    pub encrypted: bool,
}

impl FileHeader {
    /// Parse from the first 512 bytes of a `.gho`/`.ghs` file.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(Error::truncated(0, HEADER_SIZE, data.len()));
        }
        let magic = u16::from_le_bytes([data[0], data[1]]);
        if magic != GHO_MAGIC {
            return Err(Error::format(0, format!("bad file magic {magic:#06x}, expected {GHO_MAGIC:#06x}")));
        }
        let encrypted = data.len() > 12 && (data[12] & 0x02) != 0;
        Ok(Self {
            file_type: data[2],
            compression: data[3],
            image_id: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            encrypted,
        })
    }
}

/// Decoded block-level compression type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    FastLz,
    Zlib,
}

impl Compression {
    pub fn from_byte(b: u8) -> Result<Self> {
        match b {
            COMPRESSION_NONE => Ok(Self::None),
            COMPRESSION_FAST => Ok(Self::FastLz),
            3..=9 => Ok(Self::Zlib),
            other => Err(Error::UnsupportedCompression(other)),
        }
    }
}

/// Result of parsing a complete image.
#[derive(Debug)]
pub struct ParsedImage {
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
    pub output_path: std::path::PathBuf,
}

// Reserved for the streaming extract loop in the next phase.
#[allow(dead_code)]
fn _reserved() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parse_roundtrip() {
        let mut buf = [0u8; 512];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[2] = 1;
        buf[3] = 2;
        buf[4..8].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let hdr = FileHeader::parse(&buf).unwrap();
        assert_eq!(hdr.file_type, 1);
        assert_eq!(hdr.compression, 2);
        assert_eq!(hdr.image_id, 0x1234_5678);
        assert!(!hdr.encrypted);
    }

    #[test]
    fn header_detects_encryption() {
        let mut buf = [0u8; 512];
        buf[0] = 0xFE;
        buf[1] = 0xEF;
        buf[12] = 0x02;
        let hdr = FileHeader::parse(&buf).unwrap();
        assert!(hdr.encrypted);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let buf = [0u8; 512];
        let err = FileHeader::parse(&buf).unwrap_err();
        assert!(matches!(err, Error::Format { .. }));
    }

    #[test]
    fn header_rejects_truncated() {
        let err = FileHeader::parse(&[0xFE, 0xEF]).unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }));
    }

    #[test]
    fn compression_from_byte() {
        assert_eq!(Compression::from_byte(0).unwrap(), Compression::None);
        assert_eq!(Compression::from_byte(2).unwrap(), Compression::FastLz);
        assert_eq!(Compression::from_byte(3).unwrap(), Compression::Zlib);
        assert_eq!(Compression::from_byte(9).unwrap(), Compression::Zlib);
        assert!(Compression::from_byte(11).is_err());
        assert!(Compression::from_byte(255).is_err());
    }
}
